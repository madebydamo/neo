//! Settings proposals: changes the assistant suggests by option path, checked
//! against the option schema and saved only when the user clicks in the web
//! UI, through the same [`apply_edit`] as the settings form
//! (docs/design/descriptions-and-guides.md §3b).
//!
//! A change addresses an option (`path`), one named entry of it (`key`) and a
//! field inside that (`field`, dotted). Secret values never travel through the
//! assistant: `generate` runs the field's Generate script on save, `ask` puts a
//! password input on the card whose value comes with the Save click and goes
//! straight into the patch, and `copy` / `move` name another setting (`from`)
//! whose saved value is read on save, so the assistant can reorganise settings
//! with secrets inside without seeing them. Proposals hold no secret values and
//! are kept in a JSON file in neo-web's state directory, so a card survives the
//! restart that Save & apply causes.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::web::helper_exec::{parse_helper_value, run_helper_script};
use crate::commands::web::redact::{
    dropped_secrets, has_secret, is_set, redact_value, secret_in, secret_leak, Field, SET, UNSET,
};
use crate::commands::web::schema_cache::{has_placeholder, load_options};
use crate::commands::web::settings::edit::PatchOp;
use crate::commands::web::settings::save::load_settings_doc;
use crate::commands::web::settings::toml_item_to_json;
use crate::commands::web::types::{AppConfig, OptionHelper, OptionSchema, OptionType};
use crate::commands::web::util::{service_name_ok, state_dir, state_file};
use crate::utils::command::now_secs;
use crate::utils::secrets::{self, Backend, Node, Tree};
use crate::utils::secrets_store;

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
    Copy,
    Move,
}

/// The setting a `copy` / `move` reads: an option, one entry, or a field.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct SourceRef {
    pub path: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub field: Option<String>,
}

impl SourceRef {
    fn dotted(&self) -> String {
        std::iter::once(self.path.as_str())
            .chain(self.key.as_deref())
            .chain(self.field.as_deref())
            .collect::<Vec<_>>()
            .join(".")
    }
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
    /// What `copy` / `move` read.
    #[serde(default)]
    pub from: Option<SourceRef>,
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

impl Status {
    /// Waiting for the user (a failed save can be tried again).
    pub fn is_actionable(&self) -> bool {
        matches!(self, Status::Pending | Status::Failed)
    }
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
    /// Value of another setting, secrets included, read when the user saves.
    Copy {
        from: SourceRef,
        from_path: Vec<String>,
        to: Vec<String>,
        /// A move: the source goes away.
        remove_source: bool,
        /// The source has secrets inside.
        secret: bool,
    },
}

impl Planned {
    /// Settings this change depends on.
    fn paths(&self) -> Vec<&[String]> {
        match self {
            Planned::Patch(PatchOp::Set { path, .. })
            | Planned::Patch(PatchOp::Unset { path, .. })
            | Planned::Generate { path, .. }
            | Planned::Ask { path, .. } => vec![path],
            Planned::Copy { from_path, to, .. } => vec![to, from_path],
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

fn live(p: &Proposal) -> bool {
    now_secs().saturating_sub(p.created) < TTL
}

fn store_path() -> Option<PathBuf> {
    state_dir().map(|d| d.join(FILE))
}

/// Proposals from the state file, expired ones dropped. Unreadable → none
/// (the store then lives in memory only).
fn load_store() -> HashMap<String, Proposal> {
    let list: Vec<Proposal> = store_path()
        .and_then(|p| state_file::load_json(&p))
        .unwrap_or_default();
    list.into_iter()
        .filter(live)
        .map(|p| (p.id.clone(), p))
        .collect()
}

fn write_store(store: &HashMap<String, Proposal>) -> anyhow::Result<()> {
    use anyhow::Context;
    let path = store_path().context("no state directory for proposals")?;
    let list: Vec<&Proposal> = store.values().collect();
    let body = serde_json::to_string(&list).context("serialize proposals")?;
    state_file::write_atomic(&path, &body, 0o600)
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

/// Hash of the touched settings in settings.toml, to notice edits made between
/// proposing and saving. Only a hash: without a secrets file these settings
/// hold secret values, and proposals are written to disk.
fn fingerprint(config: &AppConfig, planned: &[Planned]) -> String {
    load_settings_doc(&config.settings_path)
        .map(|doc| fingerprint_of(&doc, planned))
        .unwrap_or_default()
}

fn fingerprint_of(doc: &toml_edit::DocumentMut, planned: &[Planned]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for path in planned.iter().flat_map(Planned::paths) {
        let mut item = Some(doc.as_item());
        for seg in path {
            item = item.and_then(|i| i.get(seg));
        }
        path.hash(&mut hasher);
        item.map(|i| i.to_string()).hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// Saved value at `path`: settings.toml with the secrets file laid over it.
/// `None` when neither has one (the setting is on its default).
fn saved_value(doc: &toml_edit::DocumentMut, tree: &Tree, path: &[String]) -> Option<Value> {
    fn overlay(base: Option<Value>, node: Option<&Node>) -> Option<Value> {
        match node {
            None => base,
            Some(Node::Leaf(s)) => Some(Value::String(s.clone())),
            Some(Node::Map(m)) => {
                let mut obj = match base {
                    Some(Value::Object(o)) => o,
                    _ => serde_json::Map::new(),
                };
                for (k, n) in m {
                    if let Some(v) = overlay(obj.remove(k), Some(n)) {
                        obj.insert(k.clone(), v);
                    }
                }
                (!obj.is_empty()).then_some(Value::Object(obj))
            }
        }
    }
    let mut item = Some(doc.as_item());
    for seg in path {
        item = item.and_then(|i| i.get(seg));
    }
    overlay(
        item.and_then(toml_item_to_json),
        secrets::lookup(tree, path),
    )
}

/// Secret paths stored at or below `path` that `value` (now written there)
/// no longer has: they leave the secrets file.
fn stale_secrets(keys: &Tree, path: &[String], value: &Value) -> Vec<Vec<String>> {
    fn walk(node: &Node, at: &mut Vec<String>, value: Option<&Value>, out: &mut Vec<Vec<String>>) {
        match node {
            Node::Leaf(_) if value.is_none() => out.push(at.clone()),
            Node::Leaf(_) => {}
            Node::Map(m) => {
                for (k, n) in m {
                    at.push(k.clone());
                    walk(n, at, value.and_then(|v| v.get(k)), out);
                    at.pop();
                }
            }
        }
    }
    let mut out = Vec::new();
    if let Some(node) = secrets::lookup(keys, path) {
        walk(node, &mut path.to_vec(), Some(value), &mut out);
    }
    out
}

/// `v` without nulls (settings.toml has none; evaluated values do).
fn without_nulls(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(_, x)| !x.is_null())
                .map(|(k, x)| (k.clone(), without_nulls(x)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(without_nulls).collect()),
        other => other.clone(),
    }
}

/// The secrets file with its key structure: an encrypted file has encrypted
/// values here, which is enough to see what exists without decrypting.
fn secret_keys(config: &AppConfig) -> anyhow::Result<Tree> {
    match secrets::backend_for(&config.settings_path) {
        Backend::Inline => Ok(Tree::new()),
        Backend::Plain(path) | Backend::Sops(path) => secrets::read_plain(&path),
    }
}

/// The setting a copy or move reads, as found when proposing.
#[derive(Debug)]
struct Source {
    pane: String,
    from: SourceRef,
    tg: Target,
    /// Saved value (encrypted values or secrets inside), else the evaluated one.
    value: Value,
}

impl Source {
    fn label(&self, pane: &str) -> String {
        if self.pane == pane {
            self.tg.label()
        } else {
            format!("{} › {}", self.pane, self.tg.label())
        }
    }

    fn shown(&self) -> Value {
        redact_value(&self.tg.t, self.tg.secret, &self.value)
    }

    fn has_secret(&self) -> bool {
        has_secret(&self.tg.t, self.tg.secret)
    }
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
    source: Option<&Source>,
    plan: &mut Plan,
) -> Result<(), String> {
    let tg = resolve(o, path, ch.key.as_deref(), ch.field.as_deref())
        .map_err(|m| format!("{}: {m}", ch.path))?;
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
    let cx = ChangeCx {
        ch,
        was_set: !tg.new_entry && is_set(&tg.current),
        tg,
    };

    if ch.from.is_some() && !matches!(ch.op, ChangeOp::Copy | ChangeOp::Move) {
        return Err(cx.at("from only works with op copy or move".into()));
    }
    let mut source_view = None;
    match ch.op {
        ChangeOp::Copy | ChangeOp::Move => {
            source_view = cx.plan_copy(source, pane, &mut view, plan)?;
        }
        ChangeOp::Generate => cx.plan_generate(&mut view, plan)?,
        ChangeOp::Ask => cx.plan_ask(index, pane, &mut view, plan)?,
        ChangeOp::Set => cx.plan_set(&mut view, plan)?,
        ChangeOp::Unset => cx.plan_unset(o, &mut view, plan)?,
        ChangeOp::Add | ChangeOp::Remove => cx.plan_add_remove(&mut view, plan)?,
    }
    plan.views.push(view);
    plan.views.extend(source_view);
    Ok(())
}

/// One change being planned: the request and the setting it resolved to.
struct ChangeCx<'a> {
    ch: &'a ChangeRequest,
    tg: Target,
    was_set: bool,
}

impl ChangeCx<'_> {
    fn at(&self, m: String) -> String {
        format!("{}: {m}", self.ch.path)
    }

    /// Position inside the option for messages: `tmr.password`.
    fn inside(&self, sub: &str) -> String {
        let mut parts: Vec<&str> = Vec::new();
        parts.extend(self.ch.key.as_deref().filter(|k| !k.is_empty()));
        parts.extend(self.ch.field.as_deref().filter(|f| !f.is_empty()));
        if !sub.is_empty() {
            parts.push(sub);
        }
        parts.join(".")
    }

    fn never(&self, sub: &str) -> String {
        let what = self.inside(sub);
        let what = if what.is_empty() {
            "this setting".to_string()
        } else {
            what
        };
        self.at(format!(
            "{what} is a secret; never send it. Use op copy or move when another setting already has the value, ask (the user types or generates it on the card) or generate."
        ))
    }

    fn set_action(&self, view: &mut ChangeView) {
        view.action = if self.tg.new_entry && self.ch.field.is_none() {
            "add"
        } else if self.was_set {
            "change"
        } else {
            "set"
        }
        .to_string();
    }

    fn type_check_value(&self, value: &Value) -> Result<(), String> {
        type_check(&self.tg.t, value).map_err(|e| {
            let w = self.inside("");
            self.at(if w.is_empty() { e } else { format!("{w}: {e}") })
        })
    }

    /// The source's card row when it moves.
    fn plan_copy(
        &self,
        source: Option<&Source>,
        pane: &str,
        view: &mut ChangeView,
        plan: &mut Plan,
    ) -> Result<Option<ChangeView>, String> {
        let (ch, tg) = (self.ch, &self.tg);
        let Some(src) = source else {
            return Err(self.at(
                "copy and move need from: the path (and key, field) of the setting to read".into(),
            ));
        };
        let from = src.from.dotted();
        if src.tg.new_entry || !is_set(&src.value) {
            return Err(self.at(format!("{from} has no value to copy")));
        }
        if tg.path.starts_with(&src.tg.path) || src.tg.path.starts_with(&tg.path) {
            return Err(self.at(format!(
                "{from} and the target overlap; name two separate settings or entries"
            )));
        }
        if let Some(sub) = secret_leak(&src.tg.t, src.tg.secret, &tg.t, tg.secret, &src.value) {
            let what = if sub.is_empty() {
                from.clone()
            } else {
                format!("{from}.{sub}")
            };
            return Err(self.at(format!(
                "{what} is a secret and the target is not, so it would become readable; secrets can only go to secret settings"
            )));
        }
        let shown = without_nulls(&src.shown());
        self.type_check_value(&shown)?;
        let moving = ch.op == ChangeOp::Move;
        self.set_action(view);
        view.new = display(&shown);
        view.secret = view.secret || src.has_secret();
        view.notes.push(format!(
            "{} {}",
            if moving { "Moved from" } else { "Copied from" },
            src.label(pane)
        ));
        let dropped = dropped_secrets(&tg.t, tg.secret, &tg.current, &shown);
        view.warnings.extend(
            dropped
                .iter()
                .map(|d| format!("Removes {}", self.inside(d))),
        );
        if tg.secret && self.was_set {
            view.warnings.push("Replaces the current secret".into());
        }
        if strip_null(&tg.t).kind == "attrsOf" {
            let cur = redact_value(&tg.t, tg.secret, &tg.current);
            let (added, changed, removed) = key_diff(&cur, &shown);
            note_entries(view, &added, &changed, &removed);
        }
        plan.planned.push(Planned::Copy {
            from: src.from.clone(),
            from_path: src.tg.path.clone(),
            to: tg.path.clone(),
            remove_source: moving,
            secret: src.has_secret(),
        });
        Ok(moving.then(|| ChangeView {
            service: src.pane.clone(),
            label: src.tg.label(),
            path: from,
            action: "remove".into(),
            old: display(&shown),
            new: format!("moved to {}", view.label),
            secret: src.has_secret(),
            warnings: vec![],
            notes: vec![],
        }))
    }

    fn plan_generate(&self, view: &mut ChangeView, plan: &mut Plan) -> Result<(), String> {
        let tg = &self.tg;
        let h = tg
            .helper
            .as_ref()
            .filter(|h| h.kind == "button" && h.inputs.is_empty() && !h.script.is_empty());
        let Some(h) = h.filter(|_| tg.secret) else {
            return Err(self.at(
                "generate only works on secrets with a Generate button; use op ask so the user enters it".into(),
            ));
        };
        self.set_action(view);
        view.new = "will be generated".into();
        plan.planned.push(Planned::Generate {
            path: tg.path.clone(),
            script: h.script.clone(),
        });
        Ok(())
    }

    fn plan_ask(
        &self,
        index: usize,
        pane: &str,
        view: &mut ChangeView,
        plan: &mut Plan,
    ) -> Result<(), String> {
        let tg = &self.tg;
        if !tg.secret {
            return Err(self.at("ask is only for secrets; use set for other values".into()));
        }
        if matches!(
            strip_null(&tg.t).kind.as_str(),
            "attrsOf" | "listOf" | "submodule"
        ) {
            return Err(self.at("ask fills one secret value; name it with key and field".into()));
        }
        let name = ask_input(self.ch.group.as_deref(), index);
        let used = format!("{pane}: {}", tg.label());
        match plan.asks.iter_mut().find(|a| a.name == name) {
            Some(a) => a.used_for.push(used),
            None => plan.asks.push(AskInput {
                name: name.clone(),
                label: tg.label(),
                used_for: vec![used],
            }),
        }
        self.set_action(view);
        view.new = "you enter it below".into();
        plan.planned.push(Planned::Ask {
            path: tg.path.clone(),
            input: name,
        });
        Ok(())
    }

    fn plan_set(&self, view: &mut ChangeView, plan: &mut Plan) -> Result<(), String> {
        let (ch, tg) = (self.ch, &self.tg);
        let Some(value) = ch.value.clone() else {
            return Err(self.at("set needs a value (use unset to remove)".into()));
        };
        if tg.secret {
            return Err(self.never(""));
        }
        if let Some(sub) = secret_in(&tg.t, false, &value) {
            return Err(self.never(&sub));
        }
        self.type_check_value(&value)?;
        let dropped = dropped_secrets(&tg.t, false, &tg.current, &value);
        if !dropped.is_empty() {
            let what = dropped
                .iter()
                .map(|d| self.inside(d))
                .collect::<Vec<_>>()
                .join(", ");
            let key = ch.key.as_deref().unwrap_or("<entry>");
            return Err(self.at(format!(
                "this would drop the secret {what}; set the other fields one by one with field instead of writing the whole entry, e.g. {}",
                field_example(&ch.path, key)
            )));
        }
        self.set_action(view);
        view.new = display(&value);
        let t = strip_null(&tg.t);
        let (added, changed, removed) = if t.kind == "attrsOf" {
            key_diff(&tg.current, &value)
        } else {
            Default::default()
        };
        note_entries(view, &added, &changed, &removed);
        if !removed.is_empty() {
            view.action = "change".into();
        }
        let cleanup = t.elem.as_deref().is_some_and(|e| has_secret(e, false));
        plan.planned.push(Planned::Patch(PatchOp::Set {
            path: tg.path.clone(),
            value,
        }));
        // A removed entry's secrets leave the secrets file too.
        if cleanup {
            for k in removed {
                let mut p = tg.path.clone();
                p.push(k);
                plan.planned.push(Planned::Patch(PatchOp::Unset {
                    path: p,
                    secret: true,
                }));
            }
        }
        Ok(())
    }

    fn plan_unset(
        &self,
        o: &OptionSchema,
        view: &mut ChangeView,
        plan: &mut Plan,
    ) -> Result<(), String> {
        let (ch, tg) = (self.ch, &self.tg);
        if tg.new_entry {
            let entries = o
                .current
                .as_ref()
                .and_then(|c| c.as_object())
                .map(|m| m.keys().cloned().collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            return Err(self.at(format!(
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
        plan.planned.push(Planned::Patch(PatchOp::Unset {
            secret: has_secret(&tg.t, tg.secret),
            path: tg.path.clone(),
        }));
        Ok(())
    }

    fn plan_add_remove(&self, view: &mut ChangeView, plan: &mut Plan) -> Result<(), String> {
        let (ch, tg) = (self.ch, &self.tg);
        let Some(value) = ch.value.clone() else {
            return Err(self.at("add/remove need a value".into()));
        };
        let t = strip_null(&tg.t);
        if t.kind != "listOf" || tg.secret {
            return Err(self.at("add/remove only work on lists".into()));
        }
        let elem = t.elem.as_deref().cloned().unwrap_or_default();
        if let Some(sub) = secret_in(&elem, false, &value) {
            return Err(self.never(&sub));
        }
        type_check(&elem, &value).map_err(|m| self.at(m))?;
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
        plan.planned.push(Planned::Patch(PatchOp::Set {
            path: tg.path.clone(),
            value: Value::Array(list),
        }));
        Ok(())
    }
}

/// Card lines for entries an attrsOf gains, changes or loses.
fn note_entries(view: &mut ChangeView, added: &[String], changed: &[String], removed: &[String]) {
    view.notes
        .extend(added.iter().map(|k| format!("Adds entry {k}")));
    view.notes
        .extend(changed.iter().map(|k| format!("Changes entry {k}")));
    view.warnings
        .extend(removed.iter().map(|k| format!("Removes entry {k}")));
}

/// Options of a pane, read once per proposal.
async fn load_pane_once(
    config: &AppConfig,
    panes: &mut HashMap<(bool, String), Vec<OptionSchema>>,
    is_core: bool,
    pane: &str,
) -> Result<(), String> {
    if let Entry::Vacant(e) = panes.entry((is_core, pane.to_string())) {
        e.insert(load_options(config, is_core, pane).await?);
    }
    Ok(())
}

/// Find what a copy or move reads. The options are already in `panes`.
fn find_source(
    panes: &HashMap<(bool, String), Vec<OptionSchema>>,
    doc: &toml_edit::DocumentMut,
    keys: &Tree,
    from: &SourceRef,
) -> Result<Source, String> {
    let (is_core, pane, name, prefix) = locate(&from.path)?;
    let at = |m: String| format!("from {}: {m}", from.path);
    let opts = panes
        .get(&(is_core, pane.clone()))
        .ok_or_else(|| at("could not read settings".into()))?;
    let o = opts
        .iter()
        .find(|o| o.name == name)
        .ok_or_else(|| at("no such setting".into()))?;
    let mut path = prefix;
    path.extend(name.split('.').map(str::to_string));
    let tg = resolve(o, path, from.key.as_deref(), from.field.as_deref()).map_err(at)?;
    let value = saved_value(doc, keys, &tg.path).unwrap_or_else(|| without_nulls(&tg.current));
    Ok(Source {
        pane,
        from: from.clone(),
        tg,
        value,
    })
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
    // What copies and moves read: only touched when a change has `from`.
    let saved = if req.changes.iter().any(|c| c.from.is_some()) {
        match (
            load_settings_doc(&config.settings_path),
            secret_keys(config),
        ) {
            (Ok(doc), Ok(keys)) => Some((doc, keys)),
            _ => {
                errors.push("could not read the saved settings".to_string());
                None
            }
        }
    } else {
        None
    };

    for (index, ch) in req.changes.iter().enumerate() {
        let from_refused = ch.from.as_ref().is_some_and(|f| refused(&f.path));
        if refused(&ch.path) || from_refused {
            errors.push(format!(
                "{}: the assistant may not change this setting; ask the user to change it in the web UI",
                if from_refused {
                    ch.from.as_ref().map_or("", |f| f.path.as_str())
                } else {
                    ch.path.as_str()
                }
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
        if let Err(e) = load_pane_once(config, &mut panes, is_core, &pane).await {
            errors.push(format!("{}: could not read settings ({e})", ch.path));
            continue;
        }
        let mut source = None;
        if let (Some(from), Some((doc, keys))) = (ch.from.as_ref(), saved.as_ref()) {
            let found = match locate(&from.path) {
                Ok((from_core, from_pane, ..)) => {
                    load_pane_once(config, &mut panes, from_core, &from_pane)
                        .await
                        .map_err(|e| format!("from {}: could not read settings ({e})", from.path))
                }
                Err(e) => Err(e),
            }
            .and_then(|()| find_source(&panes, doc, keys, from));
            match found {
                Ok(s) => source = Some(s),
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            }
        }
        let Some(opts) = panes.get(&(is_core, pane.clone())) else {
            continue;
        };
        let Some(o) = opts.iter().find(|o| o.name == name) else {
            errors.push(format!("{}: no such setting", ch.path));
            continue;
        };
        let mut path: Vec<String> = prefix;
        path.extend(name.split('.').map(str::to_string));
        if let Err(e) = plan_change(ch, index, &pane, o, path, source.as_ref(), &mut plan) {
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
        created: now_secs(),
    };
    put(p.clone());
    Ok(p)
}

/// Patch ops for saving `p` now with the values the user entered on the card
/// (`asked`, by input name): refuses when an asked secret is empty (the card
/// stays pending) or when the touched settings changed since proposing
/// (stale); fills generated secrets and reads what copies and moves carry.
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
    let mut stale = Vec::new();
    let copies: Vec<&Vec<String>> = p
        .planned
        .iter()
        .filter_map(|i| match i {
            Planned::Copy { from_path, .. } => Some(from_path),
            _ => None,
        })
        .collect();
    let saved = if copies.is_empty() {
        None
    } else {
        Some(read_saved(config, &copies)?)
    };
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
            Planned::Copy {
                from,
                from_path,
                to,
                remove_source,
                secret,
            } => {
                let Some(saved) = saved.as_ref() else {
                    continue;
                };
                let value = copied_value(config, saved, from, from_path).await?;
                let leaves = stale_secrets(&saved.keys, to, &value);
                stale.extend(leaves.into_iter().map(|leaf| (leaf, ops.len())));
                ops.push(PatchOp::Set {
                    path: to.clone(),
                    value,
                });
                if *remove_source {
                    ops.push(PatchOp::Unset {
                        path: from_path.clone(),
                        secret: *secret,
                    });
                }
            }
        }
    }
    let unsets = stale_unsets(stale, &ops);
    ops.extend(unsets);
    Ok(ops)
}

/// Unset ops for secrets a copy left out, `(leaf, index of the copy's op)`,
/// except those a later change of the proposal fills again.
fn stale_unsets(stale: Vec<(Vec<String>, usize)>, ops: &[PatchOp]) -> Vec<PatchOp> {
    stale
        .into_iter()
        .filter(|(leaf, copy)| {
            !ops[copy + 1..].iter().any(
                |o| matches!(o, PatchOp::Set { path, .. } if leaf.starts_with(path.as_slice())),
            )
        })
        .map(|(path, _)| PatchOp::Unset { path, secret: true })
        .collect()
}

/// Saved settings for the copies of one save.
struct Saved {
    doc: toml_edit::DocumentMut,
    /// Secrets file, decrypted only when a copy needs a secret from it.
    tree: Tree,
    keys: Tree,
}

fn read_saved(config: &AppConfig, copies: &[&Vec<String>]) -> Result<Saved, (Status, String)> {
    let unreadable = || {
        (
            Status::Failed,
            "Could not read the saved settings. Nothing was changed.".to_string(),
        )
    };
    let doc = load_settings_doc(&config.settings_path).map_err(|_| unreadable())?;
    let keys = secret_keys(config).map_err(|e| {
        eprintln!("web: proposal cannot read the secrets file: {e:#}");
        unreadable()
    })?;
    let needs_secret = copies
        .iter()
        .any(|path| secrets::lookup(&keys, path).is_some());
    let tree = match secrets::backend_for(&config.settings_path) {
        Backend::Sops(path) if needs_secret => secrets_store::decrypt(&path).map_err(|e| {
            eprintln!("web: proposal cannot decrypt the secrets: {e:#}");
            unreadable()
        })?,
        _ => keys.clone(),
    };
    Ok(Saved { doc, tree, keys })
}

/// What a copy writes: the saved value, or the evaluated one when the source
/// is on its default. Never an encrypted placeholder.
async fn copied_value(
    config: &AppConfig,
    saved: &Saved,
    from: &SourceRef,
    from_path: &[String],
) -> Result<Value, (Status, String)> {
    let dotted = from.dotted();
    let value = match saved_value(&saved.doc, &saved.tree, from_path) {
        Some(v) => Some(v),
        None => {
            let (is_core, pane, name, prefix) =
                locate(&from.path).map_err(|e| (Status::Stale, e))?;
            let opts = load_options(config, is_core, &pane)
                .await
                .map_err(|e| (Status::Failed, e))?;
            opts.iter().find(|o| o.name == name).and_then(|o| {
                let mut path = prefix;
                path.extend(name.split('.').map(str::to_string));
                let tg = resolve(o, path, from.key.as_deref(), from.field.as_deref()).ok()?;
                (!tg.new_entry && is_set(&tg.current)).then_some(tg.current)
            })
        }
    };
    let Some(value) = value.map(|v| without_nulls(&v)) else {
        return Err((
            Status::Stale,
            format!("{dotted} has no value any more. Ask the assistant to suggest this again."),
        ));
    };
    if has_placeholder(&value) {
        return Err((
            Status::Failed,
            format!("Could not read the secrets of {dotted}. Nothing was changed."),
        ));
    }
    Ok(value)
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
            plan_change(
                &change(c),
                i,
                "x",
                &o,
                p("services.x.accounts"),
                None,
                &mut plan,
            )?;
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

    /// Plan a copy/move on `accounts` of service `x` from `accounts` of `y`
    /// (`entry` there), as read from the saved settings.
    fn plan_from(c: Value, pane: &str, entry: &str) -> Result<Plan, String> {
        let o = accounts();
        let prefix = format!("services.{pane}.accounts");
        let tg = resolve(&o, p(&prefix), Some(entry), None)?;
        let value = tg.current.clone();
        let src = Source {
            pane: pane.to_string(),
            from: SourceRef {
                path: prefix,
                key: Some(entry.to_string()),
                field: None,
            },
            tg,
            value,
        };
        let mut plan = Plan::default();
        plan_change(
            &change(c),
            0,
            "x",
            &o,
            p("services.x.accounts"),
            Some(&src),
            &mut plan,
        )?;
        Ok(plan)
    }

    #[test]
    fn copy_an_entry_keeps_secrets_off_the_card() {
        let c = json!({"op":"copy","path":"services.x.accounts","key":"biel","from":{"path":"services.y.accounts","key":"tinyauth"}});
        let Plan { views, planned, .. } = plan_from(c, "y", "tinyauth").unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].label, "Accounts › biel");
        assert_eq!(views[0].action, "add");
        assert!(views[0].secret);
        assert!(views[0].new.contains("username: admin"), "{}", views[0].new);
        assert!(views[0].new.contains("password: set"), "{}", views[0].new);
        assert_eq!(views[0].notes, vec!["Copied from y › Accounts › tinyauth"]);
        match &planned[0] {
            Planned::Copy {
                from_path,
                to,
                remove_source,
                secret,
                ..
            } => {
                assert_eq!(from_path, &p("services.y.accounts.tinyauth"));
                assert_eq!(to, &p("services.x.accounts.biel"));
                assert!(!remove_source);
                assert!(secret);
            }
            other => panic!("{other:?}"),
        }
        // Nothing the assistant gets back or the store keeps has the value.
        let text = serde_json::to_string(&(&views, &planned)).unwrap();
        assert!(!text.contains("thispassword"), "{text}");
    }

    #[test]
    fn rename_is_a_move_inside_one_setting() {
        let c = json!({"op":"move","path":"services.x.accounts","key":"main","from":{"path":"services.x.accounts","key":"tinyauth"}});
        let Plan { views, planned, .. } = plan_from(c, "x", "tinyauth").unwrap();
        assert_eq!(views[0].action, "add");
        assert_eq!(views[0].notes, vec!["Moved from Accounts › tinyauth"]);
        assert_eq!(views[1].action, "remove");
        assert_eq!(views[1].label, "Accounts › tinyauth");
        assert_eq!(views[1].new, "moved to Accounts › main");
        assert!(matches!(
            &planned[0],
            Planned::Copy {
                remove_source: true,
                ..
            }
        ));
    }

    #[test]
    fn copy_refusals() {
        // Onto itself or inside itself.
        let c = json!({"op":"copy","path":"services.x.accounts","key":"tinyauth","from":{"path":"services.x.accounts","key":"tinyauth"}});
        assert!(plan_from(c, "x", "tinyauth")
            .unwrap_err()
            .contains("overlap"));
        // Nothing there.
        let c = json!({"op":"copy","path":"services.x.accounts","key":"b","from":{"path":"services.y.accounts","key":"nope"}});
        assert!(plan_from(c, "y", "nope").unwrap_err().contains("no value"));
        // Without a source, and from on another op.
        let e = plan(vec![
            json!({"op":"copy","path":"services.x.accounts","key":"b"}),
        ])
        .unwrap_err();
        assert!(e.contains("need from"), "{e}");
        let e = plan(vec![json!({"op":"set","path":"services.x.accounts","key":"b","field":"notes","value":"n","from":{"path":"services.y.accounts"}})]).unwrap_err();
        assert!(e.contains("from only works"), "{e}");
        // A whole entry into a text field: the password would become readable.
        let c = json!({"op":"copy","path":"services.x.accounts","key":"b","field":"notes","from":{"path":"services.y.accounts","key":"tinyauth"}});
        let e = plan_from(c, "y", "tinyauth").unwrap_err();
        assert!(e.contains("is a secret and the target is not"), "{e}");
        assert!(!e.contains("thispassword"), "{e}");
    }

    #[test]
    fn a_secret_field_can_be_copied_on_its_own() {
        let o = accounts();
        let tg = resolve(
            &o,
            p("services.y.accounts"),
            Some("tinyauth"),
            Some("password"),
        )
        .unwrap();
        let src = Source {
            pane: "y".into(),
            from: SourceRef {
                path: "services.y.accounts".into(),
                key: Some("tinyauth".into()),
                field: Some("password".into()),
            },
            value: tg.current.clone(),
            tg,
        };
        let mut plan = Plan::default();
        let to = |field: &str| {
            change(
                json!({"op":"copy","path":"services.x.accounts","key":"b","field":field,"from":{"path":"services.y.accounts","key":"tinyauth","field":"password"}}),
            )
        };
        plan_change(
            &to("password"),
            0,
            "x",
            &o,
            p("services.x.accounts"),
            Some(&src),
            &mut plan,
        )
        .unwrap();
        assert_eq!(plan.views[0].new, "set");
        assert!(plan.views[0].secret);
        // Into a plain field it would be readable with neo_settings_get.
        let e = plan_change(
            &to("notes"),
            1,
            "x",
            &o,
            p("services.x.accounts"),
            Some(&src),
            &mut plan,
        )
        .unwrap_err();
        assert!(e.contains("is a secret and the target is not"), "{e}");
    }

    #[test]
    fn saved_value_lays_secrets_over_settings() {
        let doc: toml_edit::DocumentMut =
            "[services.y.accounts.tinyauth]\nusername = \"admin\"\nport = 8\n"
                .parse()
                .unwrap();
        let tree = secrets::parse("services:\n  y:\n    accounts:\n      tinyauth:\n        password: \"pw\"\n      only:\n        password: \"s\"\n").unwrap();
        let v = saved_value(&doc, &tree, &p("services.y.accounts.tinyauth")).unwrap();
        assert_eq!(v, json!({"username": "admin", "port": 8, "password": "pw"}));
        // Only in the secrets file, only in settings, and nowhere.
        assert_eq!(
            saved_value(&doc, &tree, &p("services.y.accounts.only.password")),
            Some(json!("s"))
        );
        assert_eq!(
            saved_value(&doc, &tree, &p("services.y.accounts.tinyauth.username")),
            Some(json!("admin"))
        );
        assert_eq!(saved_value(&doc, &tree, &p("services.y.nope")), None);
    }

    #[test]
    fn stored_proposals_never_hold_inline_secrets() {
        // Without a secrets file the source of a copy holds the plain value.
        let doc: toml_edit::DocumentMut = "[services.y.accounts.a]\npassword = \"thispassword\"\n"
            .parse()
            .unwrap();
        let copy = Planned::Copy {
            from: SourceRef {
                path: "services.y.accounts".into(),
                key: Some("a".into()),
                field: None,
            },
            from_path: p("services.y.accounts.a"),
            to: p("services.x.accounts.a"),
            remove_source: false,
            secret: true,
        };
        let planned = vec![copy];
        let fp = fingerprint_of(&doc, &planned);
        assert!(!fp.contains("thispassword"), "{fp}");
        assert!(!serde_json::to_string(&planned)
            .unwrap()
            .contains("thispassword"));
        // It still notices a change, also of the source only.
        assert_eq!(fp, fingerprint_of(&doc, &planned));
        let changed: toml_edit::DocumentMut = "[services.y.accounts.a]\npassword = \"other\"\n"
            .parse()
            .unwrap();
        assert_ne!(fp, fingerprint_of(&changed, &planned));
    }

    #[test]
    fn stale_secrets_stay_cleared_unless_refilled_later() {
        let set = |path: &str| PatchOp::Set {
            path: p(path),
            value: json!("v"),
        };
        let leaf = p("services.x.a.totpSecret");
        // The copy's own write covers the leaf and must not keep it alive.
        let ops = vec![set("services.x.a")];
        assert_eq!(
            stale_unsets(vec![(leaf.clone(), 0)], &ops),
            vec![PatchOp::Unset {
                path: leaf.clone(),
                secret: true
            }]
        );
        // A later ask or generate for it keeps the new value.
        let ops = vec![set("services.x.a"), set("services.x.a.totpSecret")];
        assert!(stale_unsets(vec![(leaf.clone(), 0)], &ops).is_empty());
        // An earlier write is replaced by the copy anyway.
        let ops = vec![set("services.x.a.totpSecret"), set("services.x.a")];
        assert_eq!(stale_unsets(vec![(leaf, 1)], &ops).len(), 1);
    }

    #[test]
    fn copying_over_an_entry_clears_secrets_the_source_lacks() {
        use crate::commands::web::settings::edit::apply_ops;
        let keys = secrets::parse(
            "services:\n  x:\n    a:\n      password: \"old\"\n      totpSecret: \"old\"\n",
        )
        .unwrap();
        let to = p("services.x.a");
        let value = json!({"username": "u", "password": "new"});
        let stale = stale_secrets(&keys, &to, &value);
        assert_eq!(stale, vec![p("services.x.a.totpSecret")]);
        // End to end on the document and secrets tree, like save_patch_inner.
        let mut doc: toml_edit::DocumentMut = "[services.y.a]\nusername = \"u\"\n".parse().unwrap();
        let mut tree = keys.clone();
        let patterns = vec![p("services.*.a.password"), p("services.*.a.totpSecret")];
        let mut ops = vec![PatchOp::Set {
            path: to.clone(),
            value,
        }];
        ops.extend(stale.iter().map(|s| PatchOp::Unset {
            path: s.clone(),
            secret: true,
        }));
        apply_ops(&mut doc, &ops).unwrap();
        secrets::split_into(&mut doc, &mut tree, &patterns, None);
        for s in &stale {
            secrets::split_into(&mut doc, &mut tree, &patterns, Some(s));
        }
        assert!(!doc.to_string().contains("new"));
        let yaml = secrets::emit(&tree);
        assert!(
            yaml.contains("new") && !yaml.contains("totpSecret"),
            "{yaml}"
        );
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
            created: now_secs(),
        };
        let text = serde_json::to_string(&vec![&pr]).unwrap();
        let back: Vec<Proposal> = serde_json::from_str(&text).unwrap();
        assert_eq!(back[0].planned.len(), 2);
        assert!(live(&back[0]));
        let old = Proposal {
            created: now_secs() - TTL - 1,
            ..back[0].clone()
        };
        assert!(!live(&old));
    }
}
