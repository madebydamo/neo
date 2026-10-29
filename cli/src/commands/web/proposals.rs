//! Settings proposals: changes the assistant suggests by option path, checked
//! against the option schema and saved only when the user clicks in the web
//! UI, through the same [`apply_edit`] as the settings form
//! (docs/design/descriptions-and-guides.md §3b).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::commands::web::helper_exec::{parse_helper_value, run_helper_script};
use crate::commands::web::schema_cache::load_options;
use crate::commands::web::settings::edit::PatchOp;
use crate::commands::web::settings::save::load_settings_doc;
use crate::commands::web::types::{AppConfig, OptionSchema, OptionType};
use crate::commands::web::util::service_name_ok;

/// Proposals expire after a day.
const TTL: Duration = Duration::from_secs(24 * 3600);

/// Top-level settings sections (besides services / core) with their own pane.
const TOP_SECTIONS: &[&str] = &["neo-cli", "disko"];

/// Settings the assistant may not change: they could lock the user out or
/// rewrite the machine. The user changes them in the UI.
const REFUSED: &[&str] = &[
    "core.plugins",
    "core.secrets",
    "core.ssh.authorizedKeys",
    "core.hashedLinuxPassword",
    "core.uid",
    "core.gid",
    "disko",
    "neo-cli",
    "services.neo",
    "services.tinyauth.enabled",
    "services.tinyauth.users",
];

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChangeOp {
    Set,
    Unset,
    Add,
    Remove,
    Generate,
}

/// One change as the assistant sends it.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ChangeRequest {
    pub op: ChangeOp,
    /// Option path, e.g. `services.pihole.localIP` or `core.timeZone`.
    pub path: String,
    /// Entry name for attrsOf options (`set`/`unset`/`remove` one entry).
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ProposalRequest {
    pub summary: String,
    pub changes: Vec<ChangeRequest>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Saved,
    Applied,
    Rejected,
    Stale,
    Failed,
}

/// What the card shows for one change.
#[derive(Serialize, Clone, Debug)]
pub struct ChangeView {
    pub service: String,
    pub label: String,
    pub path: String,
    pub old: String,
    pub new: String,
    pub secret: bool,
}

#[derive(Clone, Debug)]
enum Planned {
    Patch(PatchOp),
    /// Secret filled by the option's own helper script when the user saves.
    Generate {
        path: Vec<String>,
        script: String,
    },
}

#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: String,
    pub summary: String,
    pub views: Vec<ChangeView>,
    planned: Vec<Planned>,
    fingerprint: String,
    pub status: Status,
    pub message: String,
    created: Instant,
}

static STORE: LazyLock<Mutex<HashMap<String, Proposal>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn new_id() -> String {
    let mut buf = [0u8; 8];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut buf))
        .is_err()
    {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        buf.copy_from_slice(&(n as u64).to_le_bytes());
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn get(id: &str) -> Option<Proposal> {
    let mut store = STORE.lock().ok()?;
    store.retain(|_, p| p.created.elapsed() < TTL);
    store.get(id).cloned()
}

fn put(p: Proposal) {
    if let Ok(mut store) = STORE.lock() {
        store.insert(p.id.clone(), p);
    }
}

pub fn set_status(id: &str, status: Status, message: &str) -> Option<Proposal> {
    let mut store = STORE.lock().ok()?;
    let p = store.get_mut(id)?;
    p.status = status;
    p.message = message.to_string();
    Some(p.clone())
}

/// `(is_core, pane name, option name inside the pane, settings path prefix)`.
fn locate(path: &str) -> Result<(bool, String, String, Vec<String>), String> {
    let segs: Vec<&str> = path.split('.').collect();
    match segs.as_slice() {
        ["services", svc, rest @ ..] if !rest.is_empty() => {
            if !service_name_ok(svc) {
                return Err(format!("{path}: unknown service"));
            }
            Ok((
                false,
                svc.to_string(),
                rest.join("."),
                vec!["services".into(), svc.to_string()],
            ))
        }
        ["core", rest @ ..] if !rest.is_empty() => {
            Ok((true, "core".into(), rest.join("."), vec!["core".into()]))
        }
        [top, rest @ ..] if TOP_SECTIONS.contains(top) && !rest.is_empty() => {
            Ok((true, top.to_string(), rest.join("."), vec![top.to_string()]))
        }
        _ => Err(format!(
            "{path}: use services.<service>.<option> or core.<option>"
        )),
    }
}

fn refused(path: &str) -> bool {
    REFUSED
        .iter()
        .any(|r| path == *r || path.starts_with(&format!("{r}.")))
}

/// Whether `value` fits the option type (shape, enum members, ranges).
pub fn type_check(t: &OptionType, value: &serde_json::Value) -> Result<(), String> {
    use serde_json::Value as V;
    let elem = || t.elem.as_deref();
    match (t.kind.as_str(), value) {
        ("nullOr", V::Null) => Ok(()),
        ("nullOr", v) => elem().map_or(Ok(()), |e| type_check(e, v)),
        ("bool", V::Bool(_)) => Ok(()),
        ("int" | "port" | "float", V::Number(n)) => {
            let x = n.as_f64().unwrap_or_default();
            if t.kind == "port" && !(0.0..=65535.0).contains(&x) {
                return Err("must be a port number (0–65535)".into());
            }
            if let Some(min) = t.min {
                if x < min as f64 {
                    return Err(format!("must be at least {min}"));
                }
            }
            if let Some(max) = t.max {
                if x > max as f64 {
                    return Err(format!("must be at most {max}"));
                }
            }
            if t.kind != "float" && n.as_i64().is_none() {
                return Err("must be a whole number".into());
            }
            Ok(())
        }
        ("str" | "strMatching" | "path", V::String(_)) => Ok(()),
        ("enum", v) => {
            let ok = t.values.as_ref().is_some_and(|vs| vs.contains(v));
            if ok {
                Ok(())
            } else {
                Err(format!(
                    "must be one of {}",
                    t.values
                        .as_ref()
                        .map(|vs| vs
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(", "))
                        .unwrap_or_default()
                ))
            }
        }
        ("listOf", V::Array(items)) => {
            if let Some(e) = elem() {
                for (i, item) in items.iter().enumerate() {
                    type_check(e, item).map_err(|m| format!("item {}: {m}", i + 1))?;
                }
            }
            Ok(())
        }
        ("attrsOf", V::Object(map)) => {
            if let Some(e) = elem() {
                for (k, v) in map {
                    type_check(e, v).map_err(|m| format!("{k}: {m}"))?;
                }
            }
            Ok(())
        }
        ("submodule", V::Object(map)) => {
            let fields = t.fields.as_deref().unwrap_or_default();
            for (k, v) in map {
                let Some(f) = fields.iter().find(|f| f.name == *k) else {
                    return Err(format!("unknown field {k}"));
                };
                type_check(&f.r#type, v).map_err(|m| format!("{k}: {m}"))?;
            }
            Ok(())
        }
        (kind, _) => Err(format!("expected a {}", kind_name(kind))),
    }
}

fn kind_name(kind: &str) -> &str {
    match kind {
        "bool" => "true/false value",
        "int" | "port" => "whole number",
        "float" => "number",
        "str" | "strMatching" | "path" => "text value",
        "listOf" => "list",
        "attrsOf" | "submodule" => "set of named values",
        other => other,
    }
}

fn short(v: &serde_json::Value) -> String {
    let s = match v {
        serde_json::Value::Null => "not set".to_string(),
        serde_json::Value::String(s) if s.is_empty() => "empty".to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if s.chars().count() > 120 {
        format!("{}…", s.chars().take(117).collect::<String>())
    } else {
        s
    }
}

fn current_of(o: &OptionSchema) -> serde_json::Value {
    o.current.clone().unwrap_or_else(|| o.default.clone())
}

/// Text of the touched settings in settings.toml, to notice edits made
/// between proposing and saving.
fn fingerprint(config: &AppConfig, planned: &[Planned]) -> String {
    let Ok(doc) = load_settings_doc(&config.settings_path) else {
        return String::new();
    };
    let mut out = String::new();
    for p in planned {
        let path = match p {
            Planned::Patch(PatchOp::Set { path, .. })
            | Planned::Patch(PatchOp::Unset { path, .. })
            | Planned::Generate { path, .. } => path,
        };
        let mut item = Some(doc.as_item());
        for seg in path {
            item = item.and_then(|i| i.get(seg));
        }
        out.push_str(&path.join("."));
        out.push('=');
        out.push_str(&item.map(|i| i.to_string()).unwrap_or_default());
        out.push('\n');
    }
    out
}

/// Check `req` against the option schemas and store it as a pending proposal.
/// Errors are plain sentences the assistant can act on.
pub async fn create(config: &AppConfig, req: ProposalRequest) -> Result<Proposal, Vec<String>> {
    let mut errors = Vec::new();
    let mut views = Vec::new();
    let mut planned = Vec::new();
    if req.changes.is_empty() {
        errors.push("a proposal needs at least one change".to_string());
    }
    let mut panes: HashMap<(bool, String), Vec<OptionSchema>> = HashMap::new();

    for ch in &req.changes {
        if refused(&ch.path) {
            errors.push(format!(
                "{}: the assistant may not change this setting; ask the user to change it in the web UI",
                ch.path
            ));
            continue;
        }
        let (is_core, pane, name, prefix) = match locate(&ch.path) {
            Ok(x) => x,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        if !panes.contains_key(&(is_core, pane.clone())) {
            match load_options(config, is_core, &pane).await {
                Ok(o) => {
                    panes.insert((is_core, pane.clone()), o);
                }
                Err(e) => {
                    errors.push(format!("{}: could not read settings ({e})", ch.path));
                    continue;
                }
            }
        }
        let opts = &panes[&(is_core, pane.clone())];
        let Some(o) = opts.iter().find(|o| o.name == name) else {
            errors.push(format!("{}: no such setting", ch.path));
            continue;
        };
        let mut path: Vec<String> = prefix.clone();
        path.extend(name.split('.').map(str::to_string));
        let label = if o.label.is_empty() {
            name.clone()
        } else {
            o.label.clone()
        };
        let current = current_of(o);
        let old = if o.secret {
            if current.is_null() || current == serde_json::json!("") {
                "not set".to_string()
            } else {
                "set".to_string()
            }
        } else {
            short(&current)
        };
        let view = |new: String| ChangeView {
            service: pane.clone(),
            label: label.clone(),
            path: ch.path.clone(),
            old: old.clone(),
            new,
            secret: o.secret,
        };

        match ch.op {
            ChangeOp::Generate => {
                let h = o.helper.as_ref().filter(|h| {
                    h.kind == "button" && h.inputs.is_empty() && !h.script.is_empty()
                });
                match h {
                    Some(h) if o.secret => {
                        views.push(view("will be generated".into()));
                        planned.push(Planned::Generate {
                            path,
                            script: h.script.clone(),
                        });
                    }
                    _ => errors.push(format!(
                        "{}: generate only works on secrets with a Generate button",
                        ch.path
                    )),
                }
            }
            ChangeOp::Set if o.secret => errors.push(format!(
                "{}: secrets are never sent by the assistant; use generate, or ask the user to enter it",
                ch.path
            )),
            ChangeOp::Set => {
                let Some(value) = ch.value.clone() else {
                    errors.push(format!("{}: set needs a value", ch.path));
                    continue;
                };
                match &ch.key {
                    Some(key) if o.r#type.kind == "attrsOf" => {
                        let elem = o.r#type.elem.as_deref().cloned().unwrap_or_default();
                        if let Err(e) = type_check(&elem, &value) {
                            errors.push(format!("{} [{key}]: {e}", ch.path));
                            continue;
                        }
                        views.push(view(format!("{key} = {}", short(&value))));
                        let mut p = path.clone();
                        p.push(key.clone());
                        planned.push(Planned::Patch(PatchOp::Set { path: p, value }));
                    }
                    Some(_) => errors.push(format!("{}: key only works on named entries", ch.path)),
                    None => {
                        if let Err(e) = type_check(&o.r#type, &value) {
                            errors.push(format!("{}: {e}", ch.path));
                            continue;
                        }
                        views.push(view(short(&value)));
                        planned.push(Planned::Patch(PatchOp::Set { path, value }));
                    }
                }
            }
            ChangeOp::Unset => {
                let mut p = path.clone();
                let new = match &ch.key {
                    Some(key) => {
                        p.push(key.clone());
                        format!("remove {key}")
                    }
                    None if o.secret => "cleared".to_string(),
                    None => format!("default ({})", short(&o.default)),
                };
                views.push(view(new));
                planned.push(Planned::Patch(PatchOp::Unset {
                    path: p,
                    secret: o.secret,
                }));
            }
            ChangeOp::Add | ChangeOp::Remove => {
                let Some(value) = ch.value.clone() else {
                    errors.push(format!("{}: add/remove need a value", ch.path));
                    continue;
                };
                if o.r#type.kind != "listOf" || o.secret {
                    errors.push(format!("{}: add/remove only work on lists", ch.path));
                    continue;
                }
                let elem = o.r#type.elem.as_deref().cloned().unwrap_or_default();
                if let Err(e) = type_check(&elem, &value) {
                    errors.push(format!("{}: {e}", ch.path));
                    continue;
                }
                let mut list = current.as_array().cloned().unwrap_or_default();
                if ch.op == ChangeOp::Add {
                    if !list.contains(&value) {
                        list.push(value.clone());
                    }
                } else {
                    list.retain(|v| v != &value);
                }
                let verb = if ch.op == ChangeOp::Add { "add" } else { "remove" };
                views.push(view(format!("{verb} {}", short(&value))));
                planned.push(Planned::Patch(PatchOp::Set {
                    path,
                    value: serde_json::Value::Array(list),
                }));
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }
    let p = Proposal {
        id: new_id(),
        summary: req.summary.trim().to_string(),
        fingerprint: fingerprint(config, &planned),
        views,
        planned,
        status: Status::Pending,
        message: String::new(),
        created: Instant::now(),
    };
    put(p.clone());
    Ok(p)
}

/// Patch ops for saving `p` now: refuses when the touched settings changed
/// since proposing (stale), fills generated secrets.
pub async fn ops_for_save(config: &AppConfig, p: &Proposal) -> Result<Vec<PatchOp>, String> {
    if fingerprint(config, &p.planned) != p.fingerprint {
        return Err("These settings changed after the assistant suggested this. Ask it to suggest the change again.".into());
    }
    let mut ops = Vec::new();
    for item in &p.planned {
        match item {
            Planned::Patch(op) => ops.push(op.clone()),
            Planned::Generate { path, script } => {
                let r = run_helper_script(Path::new(script), "{}", &[])
                    .await
                    .map_err(|_| "Could not generate a secret".to_string())?;
                if r.exit_code != 0 || r.timed_out {
                    return Err("Could not generate a secret".into());
                }
                let value = parse_helper_value(&r.stdout)
                    .map_err(|_| "Could not generate a secret".to_string())?;
                ops.push(PatchOp::Set {
                    path: path.clone(),
                    value,
                });
            }
        }
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn t(v: serde_json::Value) -> OptionType {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn locate_maps_option_paths_to_panes() {
        let (core, pane, name, prefix) = locate("services.pihole.localIP").unwrap();
        assert!(!core);
        assert_eq!((pane.as_str(), name.as_str()), ("pihole", "localIP"));
        assert_eq!(prefix, vec!["services", "pihole"]);
        let (core, pane, name, _) = locate("core.volumes.appdata").unwrap();
        assert!(core);
        assert_eq!((pane.as_str(), name.as_str()), ("core", "volumes.appdata"));
        assert!(locate("pihole.localIP").is_err());
        assert!(locate("services.pihole").is_err());
    }

    #[test]
    fn refused_paths() {
        assert!(refused("core.plugins"));
        assert!(refused("core.ssh.authorizedKeys"));
        assert!(refused("services.tinyauth.enabled"));
        assert!(!refused("services.tinyauth.sessionExpiry"));
        assert!(!refused("services.pihole.enabled"));
    }

    #[test]
    fn type_check_shapes() {
        assert!(type_check(&t(json!({"kind":"bool"})), &json!(true)).is_ok());
        assert!(type_check(&t(json!({"kind":"bool"})), &json!("yes")).is_err());
        assert!(type_check(&t(json!({"kind":"port"})), &json!(70000)).is_err());
        assert!(type_check(&t(json!({"kind":"int"})), &json!(1.5)).is_err());
        let e = t(json!({"kind":"enum","values":["switch","boot"]}));
        assert!(type_check(&e, &json!("boot")).is_ok());
        assert!(type_check(&e, &json!("reboot")).is_err());
        let n = t(json!({"kind":"nullOr","elem":{"kind":"str"}}));
        assert!(type_check(&n, &json!(null)).is_ok());
        assert!(type_check(&n, &json!(3)).is_err());
        let l = t(json!({"kind":"listOf","elem":{"kind":"enum","values":["local","web"]}}));
        assert!(type_check(&l, &json!(["local"])).is_ok());
        assert!(type_check(&l, &json!(["moon"])).is_err());
    }
}
