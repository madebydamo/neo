//! Settings proposals: changes the assistant suggests by option path, checked
//! against the option schema and saved only when the user clicks in the web
//! UI, through the same [`apply_edit`] as the settings form
//! (docs/design/descriptions-and-guides.md §3b).
//!
//! A change addresses an option (`path`), one named entry of it (`key`) and a
//! field inside that (`field`, dotted). Secret values never travel through the
//! assistant: `generate` runs the field's Generate script on save, `ask` puts a
//! password input on the card whose value comes with the Save click and goes
//! straight into the patch. Proposals hold no secret values and are kept in a
//! JSON file in neo-web's state directory, so a card survives the restart that
//! Save & apply causes.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::web::helper_exec::{parse_helper_value, run_helper_script};
use crate::commands::web::redact::{
    dropped_secrets, has_secret, is_set, redact_value, secret_in, Field, SET, UNSET,
};
use crate::commands::web::schema_cache::load_options;
use crate::commands::web::settings::edit::PatchOp;
use crate::commands::web::settings::save::load_settings_doc;
use crate::commands::web::types::{AppConfig, OptionHelper, OptionSchema, OptionType};
use crate::commands::web::util::{service_name_ok, state_dir};

/// Proposals expire after a day (seconds).
const TTL: u64 = 24 * 3600;

/// File in the neo-web state directory.
const FILE: &str = "proposals.json";

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
    Ask,
}

/// One change as the assistant sends it.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ChangeRequest {
    pub op: ChangeOp,
    /// Option path, e.g. `services.pihole.localIP` or `core.timeZone`.
    pub path: String,
    /// Entry name for attrsOf options (`accounts` → `tmr`).
    #[serde(default)]
    pub key: Option<String>,
    /// Field inside the entry (or submodule), dotted: `password`, `oauth.clientId`.
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub value: Option<Value>,
    /// `ask` changes with the same group share one input on the card.
    #[serde(default)]
    pub group: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ProposalRequest {
    pub summary: String,
    pub changes: Vec<ChangeRequest>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
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
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChangeView {
    pub service: String,
    /// Where the change lands, e.g. `Accounts › tmr › Password`.
    pub label: String,
    /// Option path plus entry and field (`services.x.accounts.tmr.password`).
    pub path: String,
    /// `add` (new entry or list item), `change`, `set` (was not set),
    /// `remove` (entry or list item), `reset` (back to the default).
    pub action: String,
    pub old: String,
    pub new: String,
    pub secret: bool,
    /// Entries a whole-map write removes ("Removes entry tinyauth").
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Entries it adds or changes.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// A password input on the card (op `ask`), shared by changes of one group.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AskInput {
    /// Form field name (`ask_<group>`).
    pub name: String,
    pub label: String,
    /// The settings it fills, when more than one.
    pub used_for: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
enum Planned {
    Patch(PatchOp),
    /// Secret filled by the field's own helper script when the user saves.
    Generate {
        path: Vec<String>,
        script: String,
    },
    /// Secret the user enters on the card; the value comes with the save.
    Ask {
        path: Vec<String>,
        input: String,
    },
}

impl Planned {
    fn path(&self) -> &[String] {
        match self {
            Planned::Patch(PatchOp::Set { path, .. })
            | Planned::Patch(PatchOp::Unset { path, .. })
            | Planned::Generate { path, .. }
            | Planned::Ask { path, .. } => path,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Proposal {
    pub id: String,
    pub summary: String,
    pub views: Vec<ChangeView>,
    #[serde(default)]
    pub asks: Vec<AskInput>,
    planned: Vec<Planned>,
    fingerprint: String,
    pub status: Status,
    pub message: String,
    /// Unix seconds.
    created: u64,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

fn live(p: &Proposal) -> bool {
    now().saturating_sub(p.created) < TTL
}

fn store_path() -> Option<PathBuf> {
    state_dir().map(|d| d.join(FILE))
}

/// Proposals from the state file, expired ones dropped. Unreadable → none
/// (the store then lives in memory only).
fn load_store() -> HashMap<String, Proposal> {
    let list: Vec<Proposal> = store_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    list.into_iter()
        .filter(live)
        .map(|p| (p.id.clone(), p))
        .collect()
}

fn write_store(store: &HashMap<String, Proposal>) -> anyhow::Result<()> {
    use anyhow::Context;
    let path = store_path().context("no state directory for proposals")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let list: Vec<&Proposal> = store.values().collect();
    let body = serde_json::to_string(&list).context("serialize proposals")?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

fn persist(store: &HashMap<String, Proposal>) {
    if let Err(e) = write_store(store) {
        eprintln!("web: proposals kept in memory only: {e:#}");
    }
}

static STORE: LazyLock<Mutex<HashMap<String, Proposal>>> =
    LazyLock::new(|| Mutex::new(load_store()));

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
    store.retain(|_, p| live(p));
    store.get(id).cloned()
}

fn put(p: Proposal) {
    if let Ok(mut store) = STORE.lock() {
        store.retain(|_, p| live(p));
        store.insert(p.id.clone(), p);
        persist(&store);
    }
}

pub fn set_status(id: &str, status: Status, message: &str) -> Option<Proposal> {
    let mut store = STORE.lock().ok()?;
    let p = store.get_mut(id)?;
    p.status = status;
    p.message = message.to_string();
    let p = p.clone();
    persist(&store);
    Some(p)
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
pub fn type_check(t: &OptionType, value: &Value) -> Result<(), String> {
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
        ("str" | "strMatching" | "separatedString" | "path", V::String(_)) => Ok(()),
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
                match crate::commands::web::redact::field(fields, k) {
                    Field::One(f) => type_check(&f.r#type, v),
                    Field::Group(g) => type_check(&g, v),
                    Field::Unknown => {
                        Err(format!("unknown field (fields: {})", field_names(fields)))
                    }
                }
                .map_err(|m| format!("{k}: {m}"))?;
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
        "str" | "strMatching" | "separatedString" | "path" => "text value",
        "listOf" => "list",
        "attrsOf" | "submodule" => "set of named values",
        other => other,
    }
}

fn field_names(fields: &[OptionSchema]) -> String {
    fields
        .iter()
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A (redacted) value as plain text for the card.
fn display(v: &Value) -> String {
    fn inner(v: &Value) -> String {
        match v {
            Value::Null => "not set".into(),
            Value::String(s) if s == SET => "set".into(),
            Value::String(s) if s == UNSET => "not set".into(),
            Value::String(s) if s.is_empty() => "empty".into(),
            Value::String(s) => s.clone(),
            Value::Array(a) if a.is_empty() => "none".into(),
            Value::Array(a) => a.iter().map(inner).collect::<Vec<_>>().join(", "),
            Value::Object(m) if m.is_empty() => "none".into(),
            Value::Object(m) => m
                .iter()
                .map(|(k, x)| match x {
                    Value::Object(_) | Value::Array(_) => format!("{k}: ({})", inner(x)),
                    _ => format!("{k}: {}", inner(x)),
                })
                .collect::<Vec<_>>()
                .join(", "),
            other => other.to_string(),
        }
    }
    let s = inner(v);
    if s.chars().count() > 120 {
        format!("{}…", s.chars().take(117).collect::<String>())
    } else {
        s
    }
}

fn current_of(o: &OptionSchema) -> Value {
    o.current.clone().unwrap_or_else(|| o.default.clone())
}

fn strip_null(t: &OptionType) -> &OptionType {
    match (t.kind.as_str(), t.elem.as_deref()) {
        ("nullOr", Some(e)) => strip_null(e),
        _ => t,
    }
}

/// What one change addresses: the option, one entry of it, or a field.
#[derive(Debug)]
struct Target {
    t: OptionType,
    secret: bool,
    helper: Option<OptionHelper>,
    /// `Accounts`, `tmr`, `Password`.
    labels: Vec<String>,
    /// Settings path (`services`, `x`, `accounts`, `tmr`, `password`).
    path: Vec<String>,
    current: Value,
    default: Value,
    /// A keyed change whose entry does not exist yet.
    new_entry: bool,
    /// Addressed below the option (key or field given).
    nested: bool,
}

impl Target {
    fn label(&self) -> String {
        self.labels.join(" › ")
    }

    fn entry(&mut self, key: &str) -> Result<(), String> {
        if key.is_empty() {
            return Err("key is empty".into());
        }
        let t = strip_null(&self.t);
        if t.kind != "attrsOf" {
            return Err("key only works on settings with named entries".into());
        }
        let elem = t.elem.as_deref().cloned().unwrap_or_default();
        match self.current.get(key) {
            Some(v) => self.current = v.clone(),
            None => {
                self.current = Value::Null;
                self.new_entry = true;
            }
        }
        self.t = elem;
        self.helper = None;
        self.default = Value::Null;
        self.path.push(key.to_string());
        self.labels.push(key.to_string());
        self.nested = true;
        Ok(())
    }

    fn field(&mut self, field: &str) -> Result<(), String> {
        let mut rest = field;
        while !rest.is_empty() {
            let t = strip_null(&self.t).clone();
            match t.kind.as_str() {
                "submodule" => {
                    let fields = t.fields.unwrap_or_default();
                    let Some(f) = fields
                        .iter()
                        .filter(|f| rest == f.name || rest.starts_with(&format!("{}.", f.name)))
                        .max_by_key(|f| f.name.len())
                    else {
                        return Err(format!(
                            "no field {rest} (fields: {})",
                            field_names(&fields)
                        ));
                    };
                    for seg in f.name.split('.') {
                        self.current = self.current.get(seg).cloned().unwrap_or(Value::Null);
                        self.path.push(seg.to_string());
                    }
                    self.t = f.r#type.clone();
                    self.secret |= f.secret;
                    self.helper = f.helper.clone();
                    self.default = f.default.clone();
                    self.labels.push(if f.label.is_empty() {
                        f.name.clone()
                    } else {
                        f.label.clone()
                    });
                    rest = rest[f.name.len()..].strip_prefix('.').unwrap_or_default();
                }
                "attrsOf" => {
                    let (key, r) = rest.split_once('.').unwrap_or((rest, ""));
                    self.entry(key)?;
                    rest = r;
                }
                _ => return Err(format!("{field}: this setting has no fields")),
            }
        }
        self.nested = true;
        Ok(())
    }
}

/// Resolve `(option, key?, field?)` to the place a change writes.
fn resolve(
    o: &OptionSchema,
    path: Vec<String>,
    key: Option<&str>,
    field: Option<&str>,
) -> Result<Target, String> {
    let mut tg = Target {
        t: o.r#type.clone(),
        secret: o.secret,
        helper: o.helper.clone(),
        labels: vec![if o.label.is_empty() {
            o.name.clone()
        } else {
            o.label.clone()
        }],
        path,
        current: current_of(o),
        default: o.default.clone(),
        new_entry: false,
        nested: false,
    };
    if let Some(key) = key {
        tg.entry(key)?;
    }
    if let Some(field) = field.filter(|f| !f.is_empty()) {
        tg.field(field)?;
    }
    Ok(tg)
}

/// Form field name for an `ask` input (group names reduced to [a-z0-9_-]).
fn ask_input(group: Option<&str>, index: usize) -> String {
    let g: String = group
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(40)
        .collect::<String>()
        .to_ascii_lowercase();
    if g.is_empty() {
        format!("ask_{index}")
    } else {
        format!("ask_g_{g}")
    }
}

/// Entries a whole-map write adds, changes and removes (keys sorted).
fn key_diff(current: &Value, new: &Value) -> (Vec<String>, Vec<String>, Vec<String>) {
    fn differs(cur: &Value, new: &Value) -> bool {
        match (cur, new) {
            (Value::Object(c), Value::Object(n)) => n
                .iter()
                .any(|(k, x)| c.get(k).is_none_or(|y| differs(y, x))),
            _ => cur != new,
        }
    }
    let empty = serde_json::Map::new();
    let cur = current.as_object().unwrap_or(&empty);
    let Some(new) = new.as_object() else {
        return Default::default();
    };
    let mut added: Vec<String> = new
        .keys()
        .filter(|k| !cur.contains_key(*k))
        .cloned()
        .collect();
    let mut changed: Vec<String> = new
        .iter()
        .filter(|(k, x)| cur.get(*k).is_some_and(|y| differs(y, x)))
        .map(|(k, _)| k.clone())
        .collect();
    let mut removed: Vec<String> = cur
        .keys()
        .filter(|k| !new.contains_key(*k))
        .cloned()
        .collect();
    added.sort();
    changed.sort();
    removed.sort();
    (added, changed, removed)
}

/// Text of the touched settings in settings.toml, to notice edits made
/// between proposing and saving.
fn fingerprint(config: &AppConfig, planned: &[Planned]) -> String {
    let Ok(doc) = load_settings_doc(&config.settings_path) else {
        return String::new();
    };
    let mut out = String::new();
    for p in planned {
        let path = p.path();
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

/// Example of setting one field of an entry, for error messages.
fn field_example(path: &str, key: &str) -> String {
    format!(
        r#"{{"op":"set","path":"{path}","key":"{key}","field":"username","value":"…"}} and {{"op":"ask","path":"{path}","key":"{key}","field":"password"}}"#
    )
}

/// Card rows, planned writes and secret inputs of a proposal being built.
#[derive(Default, Debug)]
struct Plan {
    views: Vec<ChangeView>,
    planned: Vec<Planned>,
    asks: Vec<AskInput>,
}

/// Plan one change against its option. Errors are plain sentences the
/// assistant can act on.
fn plan_change(
    ch: &ChangeRequest,
    index: usize,
    pane: &str,
    o: &OptionSchema,
    path: Vec<String>,
    plan: &mut Plan,
) -> Result<(), String> {
    let Plan {
        views,
        planned,
        asks,
    } = plan;
    let at = |m: String| format!("{}: {m}", ch.path);
    let tg = resolve(o, path, ch.key.as_deref(), ch.field.as_deref()).map_err(at)?;
    // Position inside the option for messages: `tmr.password`.
    let inside = |sub: &str| {
        let mut parts: Vec<&str> = Vec::new();
        parts.extend(ch.key.as_deref().filter(|k| !k.is_empty()));
        parts.extend(ch.field.as_deref().filter(|f| !f.is_empty()));
        if !sub.is_empty() {
            parts.push(sub);
        }
        parts.join(".")
    };
    let never = |sub: &str| {
        let what = inside(sub);
        let what = if what.is_empty() {
            "this setting".to_string()
        } else {
            what
        };
        at(format!(
            "{what} is a secret; never send it. Use op ask (the user types or generates it on the card) or generate."
        ))
    };

    let old = if tg.new_entry {
        "not set".to_string()
    } else if tg.secret {
        if is_set(&tg.current) {
            "set"
        } else {
            "not set"
        }
        .to_string()
    } else {
        display(&redact_value(&tg.t, false, &tg.current))
    };
    let was_set = !tg.new_entry && is_set(&tg.current);
    let mut view = ChangeView {
        service: pane.to_string(),
        label: tg.label(),
        path: std::iter::once(ch.path.as_str())
            .chain(ch.key.as_deref())
            .chain(ch.field.as_deref())
            .collect::<Vec<_>>()
            .join("."),
        action: String::new(),
        old,
        new: String::new(),
        secret: tg.secret,
        warnings: vec![],
        notes: vec![],
    };
    let set_action = |view: &mut ChangeView| {
        view.action = if tg.new_entry && ch.field.is_none() {
            "add"
        } else if was_set {
            "change"
        } else {
            "set"
        }
        .to_string();
    };

    match ch.op {
        ChangeOp::Generate => {
            let h = tg
                .helper
                .as_ref()
                .filter(|h| h.kind == "button" && h.inputs.is_empty() && !h.script.is_empty());
            let Some(h) = h.filter(|_| tg.secret) else {
                return Err(at(
                    "generate only works on secrets with a Generate button; use op ask so the user enters it".into(),
                ));
            };
            set_action(&mut view);
            view.new = "will be generated".into();
            planned.push(Planned::Generate {
                path: tg.path,
                script: h.script.clone(),
            });
        }
        ChangeOp::Ask => {
            if !tg.secret {
                return Err(at(
                    "ask is only for secrets; use set for other values".into()
                ));
            }
            if matches!(
                strip_null(&tg.t).kind.as_str(),
                "attrsOf" | "listOf" | "submodule"
            ) {
                return Err(at(
                    "ask fills one secret value; name it with key and field".into()
                ));
            }
            let name = ask_input(ch.group.as_deref(), index);
            let used = format!("{pane}: {}", tg.label());
            match asks.iter_mut().find(|a| a.name == name) {
                Some(a) => a.used_for.push(used),
                None => asks.push(AskInput {
                    name: name.clone(),
                    label: tg.label(),
                    used_for: vec![used],
                }),
            }
            set_action(&mut view);
            view.new = "you enter it below".into();
            planned.push(Planned::Ask {
                path: tg.path,
                input: name,
            });
        }
        ChangeOp::Set => {
            let Some(value) = ch.value.clone() else {
                return Err(at("set needs a value (use unset to remove)".into()));
            };
            if tg.secret {
                return Err(never(""));
            }
            if let Some(sub) = secret_in(&tg.t, false, &value) {
                return Err(never(&sub));
            }
            type_check(&tg.t, &value).map_err(|e| {
                let w = inside("");
                at(if w.is_empty() { e } else { format!("{w}: {e}") })
            })?;
            let dropped = dropped_secrets(&tg.t, false, &tg.current, &value);
            if !dropped.is_empty() {
                let what = dropped
                    .iter()
                    .map(|d| inside(d))
                    .collect::<Vec<_>>()
                    .join(", ");
                let key = ch.key.as_deref().unwrap_or("<entry>");
                return Err(at(format!(
                    "this would drop the secret {what}; set the other fields one by one with field instead of writing the whole entry, e.g. {}",
                    field_example(&ch.path, key)
                )));
            }
            set_action(&mut view);
            view.new = display(&value);
            let t = strip_null(&tg.t);
            let (added, changed, removed) = if t.kind == "attrsOf" {
                key_diff(&tg.current, &value)
            } else {
                Default::default()
            };
            view.notes
                .extend(added.iter().map(|k| format!("Adds entry {k}")));
            view.notes
                .extend(changed.iter().map(|k| format!("Changes entry {k}")));
            view.warnings
                .extend(removed.iter().map(|k| format!("Removes entry {k}")));
            if !removed.is_empty() {
                view.action = "change".into();
            }
            let cleanup = t.elem.as_deref().is_some_and(|e| has_secret(e, false));
            planned.push(Planned::Patch(PatchOp::Set {
                path: tg.path.clone(),
                value,
            }));
            // A removed entry's secrets leave the secrets file too.
            if cleanup {
                for k in removed {
                    let mut p = tg.path.clone();
                    p.push(k);
                    planned.push(Planned::Patch(PatchOp::Unset {
                        path: p,
                        secret: true,
                    }));
                }
            }
        }
        ChangeOp::Unset => {
            if tg.new_entry {
                let entries = o
                    .current
                    .as_ref()
                    .and_then(|c| c.as_object())
                    .map(|m| m.keys().cloned().collect::<Vec<_>>().join(", "))
                    .unwrap_or_default();
                return Err(at(format!(
                    "there is no entry {} (entries: {entries})",
                    ch.key.as_deref().unwrap_or_default()
                )));
            }
            if ch.key.is_some() && ch.field.is_none() {
                view.action = "remove".into();
                view.new = "removed".into();
            } else {
                view.action = if tg.nested { "remove" } else { "reset" }.into();
                view.new = if tg.secret {
                    "cleared".into()
                } else {
                    format!(
                        "default ({})",
                        display(&redact_value(&tg.t, false, &tg.default))
                    )
                };
            }
            planned.push(Planned::Patch(PatchOp::Unset {
                secret: has_secret(&tg.t, tg.secret),
                path: tg.path,
            }));
        }
        ChangeOp::Add | ChangeOp::Remove => {
            let Some(value) = ch.value.clone() else {
                return Err(at("add/remove need a value".into()));
            };
            let t = strip_null(&tg.t);
            if t.kind != "listOf" || tg.secret {
                return Err(at("add/remove only work on lists".into()));
            }
            let elem = t.elem.as_deref().cloned().unwrap_or_default();
            if let Some(sub) = secret_in(&elem, false, &value) {
                return Err(never(&sub));
            }
            type_check(&elem, &value).map_err(at)?;
            let mut list = tg.current.as_array().cloned().unwrap_or_default();
            if ch.op == ChangeOp::Add {
                if !list.contains(&value) {
                    list.push(value.clone());
                }
            } else {
                list.retain(|v| v != &value);
            }
            let verb = if ch.op == ChangeOp::Add {
                "add"
            } else {
                "remove"
            };
            view.action = verb.into();
            view.new = format!("{verb} {}", display(&redact_value(&elem, false, &value)));
            planned.push(Planned::Patch(PatchOp::Set {
                path: tg.path,
                value: Value::Array(list),
            }));
        }
    }
    views.push(view);
    Ok(())
}

/// Check `req` against the option schemas and store it as a pending proposal.
/// Errors are plain sentences the assistant can act on.
pub async fn create(config: &AppConfig, req: ProposalRequest) -> Result<Proposal, Vec<String>> {
    let mut errors = Vec::new();
    let mut plan = Plan::default();
    if req.changes.is_empty() {
        errors.push("a proposal needs at least one change".to_string());
    }
    let mut panes: HashMap<(bool, String), Vec<OptionSchema>> = HashMap::new();

    for (index, ch) in req.changes.iter().enumerate() {
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
        let opts = match panes.entry((is_core, pane.clone())) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => match load_options(config, is_core, &pane).await {
                Ok(o) => e.insert(o),
                Err(e) => {
                    errors.push(format!("{}: could not read settings ({e})", ch.path));
                    continue;
                }
            },
        };
        let Some(o) = opts.iter().find(|o| o.name == name) else {
            errors.push(format!("{}: no such setting", ch.path));
            continue;
        };
        let mut path: Vec<String> = prefix;
        path.extend(name.split('.').map(str::to_string));
        if let Err(e) = plan_change(ch, index, &pane, o, path, &mut plan) {
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }
    // One setting is filled once; several only list where the value goes.
    let Plan {
        views,
        planned,
        asks,
    } = plan;
    let mut asks = asks;
    for a in asks.iter_mut() {
        if a.used_for.len() < 2 {
            a.used_for.clear();
        }
    }
    let p = Proposal {
        id: new_id(),
        summary: req.summary.trim().to_string(),
        fingerprint: fingerprint(config, &planned),
        views,
        asks,
        planned,
        status: Status::Pending,
        message: String::new(),
        created: now(),
    };
    put(p.clone());
    Ok(p)
}

/// Patch ops for saving `p` now with the values the user entered on the card
/// (`asked`, by input name): refuses when an asked secret is empty (the card
/// stays pending) or when the touched settings changed since proposing
/// (stale); fills generated secrets.
pub async fn ops_for_save(
    config: &AppConfig,
    p: &Proposal,
    asked: &HashMap<String, String>,
) -> Result<Vec<PatchOp>, (Status, String)> {
    for a in &p.asks {
        if asked.get(&a.name).is_none_or(|v| v.trim().is_empty()) {
            return Err((
                Status::Pending,
                format!("Enter {} on the card (or click Generate) first.", a.label),
            ));
        }
    }
    if fingerprint(config, &p.planned) != p.fingerprint {
        return Err((Status::Stale, "These settings changed after the assistant suggested this. Ask it to suggest the change again.".into()));
    }
    let failed = || (Status::Failed, "Could not generate a secret".to_string());
    let mut ops = Vec::new();
    for item in &p.planned {
        match item {
            Planned::Patch(op) => ops.push(op.clone()),
            Planned::Generate { path, script } => {
                let r = run_helper_script(Path::new(script), "{}", &[])
                    .await
                    .map_err(|_| failed())?;
                if r.exit_code != 0 || r.timed_out {
                    return Err(failed());
                }
                let value = parse_helper_value(&r.stdout).map_err(|_| failed())?;
                ops.push(PatchOp::Set {
                    path: path.clone(),
                    value,
                });
            }
            Planned::Ask { path, input } => {
                let value = asked.get(input).cloned().unwrap_or_default();
                ops.push(PatchOp::Set {
                    path: path.clone(),
                    value: Value::String(value),
                });
            }
        }
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::web::redact::tests::accounts;
    use serde_json::json;

    fn t(v: serde_json::Value) -> OptionType {
        serde_json::from_value(v).unwrap()
    }

    fn p(s: &str) -> Vec<String> {
        s.split('.').map(str::to_string).collect()
    }

    fn change(v: Value) -> ChangeRequest {
        serde_json::from_value(v).unwrap()
    }

    /// Plan changes on the `accounts` option of service `x`.
    fn plan(changes: Vec<Value>) -> Result<Plan, String> {
        let o = accounts();
        let mut plan = Plan::default();
        for (i, c) in changes.into_iter().enumerate() {
            plan_change(&change(c), i, "x", &o, p("services.x.accounts"), &mut plan)?;
        }
        Ok(plan)
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
        // Flattened nested submodule fields accept the nested object.
        let a = accounts().r#type;
        assert!(type_check(&a, &json!({"t": {"oauth": {"clientId": "c"}}})).is_ok());
        assert!(type_check(&a, &json!({"t": {"nope": 1}})).is_err());
    }

    #[test]
    fn add_an_entry_field_by_field() {
        let Plan { views, planned, asks } = plan(vec![
            json!({"op":"set","path":"services.x.accounts","key":"tmr","field":"username","value":"tmr"}),
            json!({"op":"ask","path":"services.x.accounts","key":"tmr","field":"password","group":"tmr-password"}),
        ])
        .unwrap();
        assert_eq!(views[0].label, "Accounts › tmr › Username");
        assert_eq!(views[0].old, "not set");
        assert_eq!(views[0].action, "set");
        assert_eq!(views[1].label, "Accounts › tmr › Password");
        assert_eq!(views[1].new, "you enter it below");
        assert!(views[1].secret);
        assert_eq!(asks.len(), 1);
        assert_eq!(asks[0].name, "ask_g_tmr-password");
        match &planned[0] {
            Planned::Patch(PatchOp::Set { path, value }) => {
                assert_eq!(path, &p("services.x.accounts.tmr.username"));
                assert_eq!(value, &json!("tmr"));
            }
            other => panic!("{other:?}"),
        }
        match &planned[1] {
            Planned::Ask { path, input } => {
                assert_eq!(path, &p("services.x.accounts.tmr.password"));
                assert_eq!(input, "ask_g_tmr-password");
            }
            other => panic!("{other:?}"),
        }
        // The persisted plan holds no value for the asked secret.
        let json = serde_json::to_string(&planned).unwrap();
        assert!(!json.contains("value\":\"pw"));
    }

    #[test]
    fn secrets_are_refused_everywhere() {
        // Whole map with a plaintext password (the incident's first try).
        let e = plan(vec![
            json!({"op":"set","path":"services.x.accounts","value":{
            "tinyauth": {"username":"admin"}, "tmr": {"username":"tmr","password":"pw"}}}),
        ])
        .unwrap_err();
        assert!(e.contains("tmr.password is a secret; never send it"), "{e}");
        // One entry with a password, and the field itself.
        let e = plan(vec![json!({"op":"set","path":"services.x.accounts","key":"tmr","value":{"username":"tmr","password":"pw"}})]).unwrap_err();
        assert!(e.contains("tmr.password is a secret"), "{e}");
        let e = plan(vec![json!({"op":"set","path":"services.x.accounts","key":"tmr","field":"password","value":"pw"})]).unwrap_err();
        assert!(e.contains("tmr.password is a secret"), "{e}");
        // ask only on secrets.
        assert!(plan(vec![
            json!({"op":"ask","path":"services.x.accounts","key":"tmr","field":"username"})
        ])
        .is_err());
    }

    #[test]
    fn whole_entry_set_must_not_drop_secrets() {
        let e = plan(vec![json!({"op":"set","path":"services.x.accounts","key":"tinyauth","value":{"username":"root"}})]).unwrap_err();
        assert!(e.contains("drop the secret"), "{e}");
        assert!(e.contains("\"field\":\"username\""), "{e}");
        // A new entry without its password is fine.
        let Plan { views, .. } = plan(vec![
            json!({"op":"set","path":"services.x.accounts","key":"tmr","value":{"username":"tmr"}}),
        ])
        .unwrap();
        assert_eq!(views[0].action, "add");
        assert_eq!(views[0].label, "Accounts › tmr");
        assert_eq!(views[0].old, "not set");
        assert_eq!(views[0].new, "username: tmr");
    }

    #[test]
    fn keyed_preview_shows_the_entry_not_the_map() {
        let Plan { views, .. } = plan(vec![json!({"op":"set","path":"services.x.accounts","key":"tinyauth","field":"notes","value":"main login"})]).unwrap();
        assert_eq!(views[0].action, "set");
        assert_eq!(views[0].old, "empty");
        let Plan { views, .. } = plan(vec![json!({"op":"set","path":"services.x.accounts","key":"tinyauth","field":"username","value":"root"})]).unwrap();
        assert_eq!(views[0].action, "change");
        assert_eq!(views[0].old, "admin");
        assert!(!views[0].old.contains("thispassword"));
    }

    #[test]
    fn whole_map_set_lists_removed_entries() {
        let Plan { views, planned, .. } = plan(vec![
            json!({"op":"set","path":"services.x.accounts","value":{"tmr":{"username":"tmr"}}}),
        ])
        .unwrap();
        assert_eq!(views[0].warnings, vec!["Removes entry tinyauth"]);
        assert_eq!(views[0].notes, vec!["Adds entry tmr"]);
        assert!(!views[0].old.contains("thispassword"), "{}", views[0].old);
        // The set first, then the removed entry's secrets leave the secrets file.
        assert!(
            matches!(&planned[0], Planned::Patch(PatchOp::Set { path, .. }) if path == &p("services.x.accounts"))
        );
        assert!(
            matches!(&planned[1], Planned::Patch(PatchOp::Unset { path, secret: true }) if path == &p("services.x.accounts.tinyauth"))
        );
    }

    #[test]
    fn generate_and_unset_nested() {
        let Plan { views, planned, .. } = plan(vec![
            json!({"op":"generate","path":"services.x.accounts","key":"tmr","field":"password"}),
            json!({"op":"unset","path":"services.x.accounts","key":"tinyauth"}),
        ])
        .unwrap();
        assert_eq!(views[0].new, "will be generated");
        assert!(
            matches!(&planned[0], Planned::Generate { path, script } if path == &p("services.x.accounts.tmr.password") && script == "/nix/store/x-gen")
        );
        assert_eq!(views[1].action, "remove");
        assert!(matches!(
            &planned[1],
            Planned::Patch(PatchOp::Unset { secret: true, .. })
        ));
        // No Generate button on the TOTP secret; unknown entries cannot be removed.
        assert!(plan(vec![
            json!({"op":"generate","path":"services.x.accounts","key":"tmr","field":"totpSecret"})
        ])
        .is_err());
        assert!(plan(vec![
            json!({"op":"unset","path":"services.x.accounts","key":"nope"})
        ])
        .is_err());
        assert!(plan(vec![
            json!({"op":"set","path":"services.x.accounts","key":"tmr","field":"nope","value":1})
        ])
        .is_err());
    }

    #[test]
    fn proposals_round_trip_without_secrets() {
        let Plan { views, planned, asks } = plan(vec![
            json!({"op":"ask","path":"services.x.accounts","key":"a","field":"password","group":"g"}),
            json!({"op":"ask","path":"services.x.accounts","key":"b","field":"password","group":"g"}),
        ])
        .unwrap();
        assert_eq!(asks.len(), 1);
        assert_eq!(asks[0].used_for.len(), 2);
        let pr = Proposal {
            id: "0123456789abcdef".into(),
            summary: "s".into(),
            views,
            asks,
            planned,
            fingerprint: String::new(),
            status: Status::Pending,
            message: String::new(),
            created: now(),
        };
        let text = serde_json::to_string(&vec![&pr]).unwrap();
        let back: Vec<Proposal> = serde_json::from_str(&text).unwrap();
        assert_eq!(back[0].planned.len(), 2);
        assert!(live(&back[0]));
        let old = Proposal {
            created: now() - TTL - 1,
            ..back[0].clone()
        };
        assert!(!live(&old));
    }
}
