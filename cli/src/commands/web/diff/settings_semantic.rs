//! Semantic diff of two `settings.toml` documents: services enabled/disabled and
//! per-service option changes (old → new), with secret-looking values masked.
//! Secrets kept in secrets.yaml / secrets.sops.yaml are overlaid on their
//! settings paths, so they show as masked option changes too (and moving a
//! secret between settings.toml and a secrets file is no change).
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, Table, TableLike, Value};

use crate::utils::secrets::{Node, Tree};

/// Placeholder shown instead of secret values.
pub const MASK: &str = "••••••";

/// Max rendered length of one value before it is shortened.
const MAX_VALUE_LEN: usize = 160;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueChange {
    /// Dotted option path relative to the service / section (`llm.model`).
    pub path: String,
    pub old: Option<String>,
    pub new: Option<String>,
    /// Values were masked because the key looks like a credential.
    pub secret: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    Enabled,
    Disabled,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceDiff {
    pub name: String,
    /// `enabled` before (`None` = no `[services.<name>]` table).
    pub before: Option<bool>,
    pub after: Option<bool>,
    /// Option changes other than `enabled`.
    pub changes: Vec<ValueChange>,
}

impl ServiceDiff {
    pub fn transition(&self) -> Transition {
        let was = self.before == Some(true);
        let is = self.after == Some(true);
        match (was, is) {
            (false, true) => Transition::Enabled,
            (true, false) => Transition::Disabled,
            _ => Transition::Unchanged,
        }
    }
}

/// Non-service top-level table (`core`, `neo-cli`, `disko`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionDiff {
    pub name: String,
    pub changes: Vec<ValueChange>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsDiff {
    pub services: Vec<ServiceDiff>,
    pub sections: Vec<SectionDiff>,
    /// Caveat about the secret changes (e.g. encrypted values not comparable).
    pub secrets_note: Option<String>,
}

impl SettingsDiff {
    pub fn is_empty(&self) -> bool {
        self.services.is_empty() && self.sections.is_empty()
    }
}

/// True when a dotted key path looks like it holds a credential.
pub fn is_secret_path(path: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "password",
        "passwd",
        "passphrase",
        "secret",
        "token",
        "apikey",
        "privatekey",
        "credential",
        "auth_key",
        "authkey",
        "cookie",
        "salt",
    ];
    path.split('.').any(|seg| {
        let norm: String = seg
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect::<String>()
            .to_ascii_lowercase();
        let compact = norm.replace('_', "");
        NEEDLES
            .iter()
            .any(|n| norm.contains(n) || compact.contains(n))
            || compact == "key"
            || compact.ends_with("key")
            || compact.ends_with("keys")
    })
}

fn is_bare_key(k: &str) -> bool {
    !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn join_key(prefix: &str, key: &str) -> String {
    let k = if is_bare_key(key) {
        key.to_string()
    } else {
        format!("{key:?}")
    };
    if prefix.is_empty() {
        k
    } else {
        format!("{prefix}.{k}")
    }
}

fn render_table_like(t: &dyn TableLike) -> String {
    let parts: Vec<String> = t
        .iter()
        .filter_map(|(k, v)| {
            let val = match v {
                Item::Value(v) => render_value(v),
                Item::Table(t) => render_table_like(t),
                Item::ArrayOfTables(a) => render_array_of_tables(a.iter()),
                Item::None => return None,
            };
            Some(format!("{} = {}", join_key("", k), val))
        })
        .collect();
    if parts.is_empty() {
        "{}".to_string()
    } else {
        format!("{{ {} }}", parts.join(", "))
    }
}

fn render_array_of_tables<'a>(it: impl Iterator<Item = &'a Table>) -> String {
    let parts: Vec<String> = it.map(|t| render_table_like(t)).collect();
    format!("[{}]", parts.join(", "))
}

/// Compact, decor-free TOML rendering of a value (comments / newlines dropped).
pub fn render_value(v: &Value) -> String {
    match v {
        Value::String(s) => format!("{:?}", s.value()),
        Value::Integer(i) => i.value().to_string(),
        Value::Float(f) => f.value().to_string(),
        Value::Boolean(b) => b.value().to_string(),
        Value::Datetime(d) => d.value().to_string(),
        Value::Array(a) => {
            let parts: Vec<String> = a.iter().map(render_value).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::InlineTable(t) => render_table_like(t),
    }
}

fn shorten(s: String) -> String {
    if s.chars().count() <= MAX_VALUE_LEN {
        return s;
    }
    let cut: String = s.chars().take(MAX_VALUE_LEN).collect();
    format!("{cut}…")
}

/// Flatten tables (and inline tables) into `dotted.path → rendered value`.
fn flatten(prefix: &str, item: &Item, out: &mut BTreeMap<String, String>) {
    match item {
        Item::None => {}
        Item::Table(_) | Item::Value(Value::InlineTable(_)) => {
            if let Some(t) = item.as_table_like() {
                for (k, v) in t.iter() {
                    flatten(&join_key(prefix, k), v, out);
                }
            }
        }
        Item::Value(v) => {
            out.insert(prefix.to_string(), render_value(v));
        }
        Item::ArrayOfTables(a) => {
            out.insert(prefix.to_string(), render_array_of_tables(a.iter()));
        }
    }
}

fn flat_of(item: Option<&Item>) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    if let Some(i) = item {
        flatten("", i, &mut m);
    }
    m
}

/// Secret tree leaves below `path` as `dotted.path → comparison token`.
fn flat_secrets(tree: &Tree, path: &[&str]) -> BTreeMap<String, String> {
    fn walk(prefix: &str, tree: &Tree, out: &mut BTreeMap<String, String>) {
        for (k, n) in tree {
            let p = join_key(prefix, k);
            match n {
                // Same rendering as a TOML string, so an inline value equals its moved copy.
                Node::Leaf(v) => {
                    out.insert(p, format!("{v:?}"));
                }
                Node::Map(m) => walk(&p, m, out),
            }
        }
    }
    let mut cur = tree;
    for k in path {
        match cur.get(*k) {
            Some(Node::Map(m)) => cur = m,
            _ => return BTreeMap::new(),
        }
    }
    let mut out = BTreeMap::new();
    walk("", cur, &mut out);
    out
}

/// Settings of one scope with its secrets overlaid; returns the secret keys.
fn overlay(flat: &mut BTreeMap<String, String>, secrets: &Tree, path: &[&str]) -> BTreeSet<String> {
    let s = flat_secrets(secrets, path);
    let keys = s.keys().cloned().collect();
    flat.extend(s);
    keys
}

/// Compare two flattened maps; `secret_keys` are masked whatever their name.
fn diff_maps(
    old: &BTreeMap<String, String>,
    new: &BTreeMap<String, String>,
    scope: &str,
    skip: &[&str],
    secret_keys: &BTreeSet<String>,
) -> Vec<ValueChange> {
    let keys: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    let mut out = Vec::new();
    for k in keys {
        if skip.contains(&k.as_str()) {
            continue;
        }
        let (o, n) = (old.get(k), new.get(k));
        if o == n {
            continue;
        }
        let path = if k.is_empty() {
            scope.to_string()
        } else {
            k.clone()
        };
        let secret = secret_keys.contains(k) || is_secret_path(&path);
        let show = |v: Option<&String>| {
            v.map(|s| {
                if secret {
                    MASK.to_string()
                } else {
                    shorten(s.clone())
                }
            })
        };
        out.push(ValueChange {
            old: show(o),
            new: show(n),
            path,
            secret,
        });
    }
    out
}

fn table_keys(t: Option<&dyn TableLike>) -> Vec<String> {
    t.map(|t| t.iter().map(|(k, _)| k.to_string()).collect())
        .unwrap_or_default()
}

fn enabled_of(item: Option<&Item>) -> Option<bool> {
    let t = item?.as_table_like()?;
    Some(t.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false))
}

fn parse_doc(raw: &str, which: &str) -> Result<DocumentMut> {
    raw.parse::<DocumentMut>()
        .with_context(|| format!("parse {which} settings.toml"))
}

/// Semantic diff between two `settings.toml` texts (empty text = no file).
pub fn diff_settings_toml(old: &str, new: &str) -> Result<SettingsDiff> {
    diff_settings(old, new, &Tree::new(), &Tree::new())
}

/// Like [`diff_settings_toml`], with the secrets of each side (settings paths →
/// comparison tokens) overlaid; every secret change is masked.
pub fn diff_settings(
    old: &str,
    new: &str,
    old_secrets: &Tree,
    new_secrets: &Tree,
) -> Result<SettingsDiff> {
    let old_doc = parse_doc(old, "previous")?;
    let new_doc = parse_doc(new, "current")?;
    let old_root = old_doc.as_table();
    let new_root = new_doc.as_table();

    let old_svcs = old_root.get("services").and_then(|i| i.as_table_like());
    let new_svcs = new_root.get("services").and_then(|i| i.as_table_like());
    let secret_svcs = |t: &Tree| -> Vec<String> {
        match t.get("services") {
            Some(Node::Map(m)) => m.keys().cloned().collect(),
            _ => Vec::new(),
        }
    };
    let names: BTreeSet<String> = table_keys(old_svcs)
        .into_iter()
        .chain(table_keys(new_svcs))
        .chain(secret_svcs(old_secrets))
        .chain(secret_svcs(new_secrets))
        .collect();

    let mut services = Vec::new();
    for name in names {
        let o = old_svcs.and_then(|t| t.get(&name));
        let n = new_svcs.and_then(|t| t.get(&name));
        let (mut fo, mut fn_) = (flat_of(o), flat_of(n));
        let at = ["services", name.as_str()];
        let mut secret_keys = overlay(&mut fo, old_secrets, &at);
        secret_keys.extend(overlay(&mut fn_, new_secrets, &at));
        let changes = diff_maps(&fo, &fn_, &name, &["enabled"], &secret_keys);
        let svc = ServiceDiff {
            before: enabled_of(o),
            after: enabled_of(n),
            name,
            changes,
        };
        if svc.transition() != Transition::Unchanged || !svc.changes.is_empty() {
            services.push(svc);
        }
    }

    let sections_names: BTreeSet<String> = old_root
        .iter()
        .chain(new_root.iter())
        .map(|(k, _)| k.to_string())
        .chain(old_secrets.keys().cloned())
        .chain(new_secrets.keys().cloned())
        .filter(|k| k != "services")
        .collect();
    let mut sections = Vec::new();
    for name in sections_names {
        let (mut fo, mut fn_) = (flat_of(old_root.get(&name)), flat_of(new_root.get(&name)));
        let at = [name.as_str()];
        let mut secret_keys = overlay(&mut fo, old_secrets, &at);
        secret_keys.extend(overlay(&mut fn_, new_secrets, &at));
        let changes = diff_maps(&fo, &fn_, &name, &[], &secret_keys);
        if !changes.is_empty() {
            sections.push(SectionDiff { name, changes });
        }
    }

    Ok(SettingsDiff {
        services,
        sections,
        secrets_note: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = r#"
[core]
hostname = "box"
hashedLinuxPassword = "$6$old"

[services.jellyfin]
enabled = true
subdomain = "tv"

[services.immich]
enabled = false

[services.hermes]
enabled = true
[services.hermes.llm]
model = "a"
apiKey = "sk-old"
"#;

    const NEW: &str = r#"
[core]
hostname = "box2"
hashedLinuxPassword = "$6$new"

[services.jellyfin]
enabled = false
subdomain = "tv"

[services.immich]
enabled = true
ingress = ["local", "web"]

[services.hermes]
enabled = true
[services.hermes.llm]
model = "b"
apiKey = "sk-new"

[services.paperless]
enabled = true
"#;

    fn svc<'a>(d: &'a SettingsDiff, n: &str) -> &'a ServiceDiff {
        d.services
            .iter()
            .find(|s| s.name == n)
            .expect("service present")
    }

    #[test]
    fn transitions_and_option_changes() {
        let d = diff_settings_toml(OLD, NEW).expect("diff");
        assert_eq!(svc(&d, "jellyfin").transition(), Transition::Disabled);
        assert!(svc(&d, "jellyfin").changes.is_empty());

        let immich = svc(&d, "immich");
        assert_eq!(immich.transition(), Transition::Enabled);
        assert_eq!(immich.changes.len(), 1);
        assert_eq!(immich.changes[0].path, "ingress");
        assert_eq!(immich.changes[0].old, None);
        assert_eq!(
            immich.changes[0].new.as_deref(),
            Some(r#"["local", "web"]"#)
        );

        let paperless = svc(&d, "paperless");
        assert_eq!(paperless.before, None);
        assert_eq!(paperless.transition(), Transition::Enabled);

        let hermes = svc(&d, "hermes");
        assert_eq!(hermes.transition(), Transition::Unchanged);
        let model = hermes
            .changes
            .iter()
            .find(|c| c.path == "llm.model")
            .expect("model");
        assert_eq!(model.old.as_deref(), Some("\"a\""));
        assert_eq!(model.new.as_deref(), Some("\"b\""));
        assert!(!model.secret);
    }

    #[test]
    fn secrets_are_masked() {
        let d = diff_settings_toml(OLD, NEW).expect("diff");
        let key = svc(&d, "hermes")
            .changes
            .iter()
            .find(|c| c.path == "llm.apiKey")
            .expect("apiKey");
        assert!(key.secret);
        assert_eq!(key.old.as_deref(), Some(MASK));
        assert_eq!(key.new.as_deref(), Some(MASK));

        let core = d.sections.iter().find(|s| s.name == "core").expect("core");
        let pw = core
            .changes
            .iter()
            .find(|c| c.path == "hashedLinuxPassword")
            .expect("pw");
        assert!(pw.secret);
        let rendered = format!("{:?}", d);
        assert!(!rendered.contains("sk-new"));
        assert!(!rendered.contains("$6$new"));
    }

    #[test]
    fn unchanged_services_are_omitted_and_formatting_ignored() {
        let a = "[services.x]\nenabled = true\nlist = [1, 2]\n";
        let b = "[services.x]\nenabled = true # comment\nlist = [\n  1,\n  2,\n]\n";
        let d = diff_settings_toml(a, b).expect("diff");
        assert!(d.is_empty());
    }

    #[test]
    fn empty_old_file_and_parse_errors() {
        let d = diff_settings_toml("", "[services.x]\nenabled = true\n").expect("diff");
        assert_eq!(d.services[0].transition(), Transition::Enabled);
        assert!(diff_settings_toml("[broken", "").is_err());
    }

    fn tree(pairs: &[(&str, &str)]) -> Tree {
        let mut t = Tree::new();
        for (path, v) in pairs {
            let parts: Vec<&str> = path.split('.').collect();
            let (last, parents) = parts.split_last().expect("path");
            let mut cur = &mut t;
            for k in parents {
                let e = cur
                    .entry((*k).to_string())
                    .or_insert_with(|| Node::Map(Tree::new()));
                let Node::Map(m) = e else {
                    panic!("leaf in path")
                };
                cur = m;
            }
            cur.insert((*last).to_string(), Node::Leaf((*v).to_string()));
        }
        t
    }

    #[test]
    fn secrets_file_changes_are_masked_option_changes() {
        let settings = "[services.dufs]\nenabled = true\n[core]\nhostname = \"box\"\n";
        let old = tree(&[
            ("services.dufs.users.admin", "pw-old"),
            ("services.dufs.users.gone", "pw-gone"),
            ("core.git.tokens.github.com", "same"),
        ]);
        let new = tree(&[
            ("services.dufs.users.admin", "pw-new"),
            ("services.paperless.adminPassword", "pw-added"),
            ("core.git.tokens.github.com", "same"),
        ]);
        let d = diff_settings(settings, settings, &old, &new).expect("diff");
        let dufs = svc(&d, "dufs");
        let kinds: Vec<(&str, bool, bool)> = dufs
            .changes
            .iter()
            .map(|c| (c.path.as_str(), c.old.is_some(), c.new.is_some()))
            .collect();
        assert_eq!(
            kinds,
            vec![("users.admin", true, true), ("users.gone", true, false)]
        );
        assert!(dufs.changes.iter().all(|c| c.secret));
        let paperless = svc(&d, "paperless");
        assert_eq!(paperless.transition(), Transition::Unchanged);
        assert_eq!(paperless.changes[0].path, "adminPassword");
        assert_eq!(paperless.changes[0].old, None);
        assert_eq!(paperless.changes[0].new.as_deref(), Some(MASK));
        assert!(d.sections.is_empty(), "unchanged secret reported");
        assert!(!format!("{d:?}").contains("pw-"));
    }

    #[test]
    fn moving_a_secret_out_of_settings_is_no_change() {
        let old = "[services.dufs]\nenabled = true\nusers = { admin = \"pw\" }\n";
        let new = "[services.dufs]\nenabled = true\n";
        let moved = tree(&[("services.dufs.users.admin", "pw")]);
        let d = diff_settings(old, new, &Tree::new(), &moved).expect("diff");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn secret_heuristic() {
        for k in [
            "dbPassword",
            "jwtSecret",
            "encryptionKey",
            "llm.apiKey",
            "api_key",
            "token",
            "secrets.x",
        ] {
            assert!(is_secret_path(k), "{k} should be secret");
        }
        for k in ["subdomain", "enabled", "ingress", "llm.model", "hostname"] {
            assert!(!is_secret_path(k), "{k} should not be secret");
        }
    }
}
