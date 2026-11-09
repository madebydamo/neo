//! Reading settings for a proposal: resolving a path to an option, type checks and
//! the values a card shows.
use super::*;

/// `(is_core, pane name, option name inside the pane, settings path prefix)`.
pub(super) fn locate(path: &str) -> Result<(bool, String, String, Vec<String>), String> {
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

pub(super) fn refused(path: &str) -> bool {
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

pub(super) fn kind_name(kind: &str) -> &str {
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

pub(super) fn field_names(fields: &[OptionSchema]) -> String {
    fields
        .iter()
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A (redacted) value as plain text for the card.
pub(super) fn display(v: &Value) -> String {
    pub(super) fn inner(v: &Value) -> String {
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

pub(super) fn current_of(o: &OptionSchema) -> Value {
    o.current.clone().unwrap_or_else(|| o.default.clone())
}

pub(super) fn strip_null(t: &OptionType) -> &OptionType {
    match (t.kind.as_str(), t.elem.as_deref()) {
        ("nullOr", Some(e)) => strip_null(e),
        _ => t,
    }
}

/// What one change addresses: the option, one entry of it, or a field.
#[derive(Debug)]
pub(super) struct Target {
    pub(super) t: OptionType,
    pub(super) secret: bool,
    pub(super) helper: Option<OptionHelper>,
    /// `Accounts`, `tmr`, `Password`.
    pub(super) labels: Vec<String>,
    /// Settings path (`services`, `x`, `accounts`, `tmr`, `password`).
    pub(super) path: Vec<String>,
    pub(super) current: Value,
    pub(super) default: Value,
    /// A keyed change whose entry does not exist yet.
    pub(super) new_entry: bool,
    /// Addressed below the option (key or field given).
    pub(super) nested: bool,
}

impl Target {
    pub(super) fn label(&self) -> String {
        self.labels.join(" › ")
    }

    pub(super) fn entry(&mut self, key: &str) -> Result<(), String> {
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

    pub(super) fn field(&mut self, field: &str) -> Result<(), String> {
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
pub(super) fn resolve(
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
pub(super) fn ask_input(group: Option<&str>, index: usize) -> String {
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
pub(super) fn key_diff(current: &Value, new: &Value) -> (Vec<String>, Vec<String>, Vec<String>) {
    pub(super) fn differs(cur: &Value, new: &Value) -> bool {
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
pub(super) fn fingerprint(config: &AppConfig, planned: &[Planned]) -> String {
    load_settings_doc(&config.settings_path)
        .map(|doc| fingerprint_of(&doc, planned))
        .unwrap_or_default()
}

pub(super) fn fingerprint_of(doc: &toml_edit::DocumentMut, planned: &[Planned]) -> String {
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
pub(super) fn saved_value(
    doc: &toml_edit::DocumentMut,
    tree: &Tree,
    path: &[String],
) -> Option<Value> {
    pub(super) fn overlay(base: Option<Value>, node: Option<&Node>) -> Option<Value> {
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
pub(super) fn stale_secrets(keys: &Tree, path: &[String], value: &Value) -> Vec<Vec<String>> {
    pub(super) fn walk(
        node: &Node,
        at: &mut Vec<String>,
        value: Option<&Value>,
        out: &mut Vec<Vec<String>>,
    ) {
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
pub(super) fn without_nulls(v: &Value) -> Value {
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
pub(super) fn secret_keys(config: &AppConfig) -> anyhow::Result<Tree> {
    match secrets::backend_for(&config.settings_path) {
        Backend::Inline => Ok(Tree::new()),
        Backend::Plain(path) | Backend::Sops(path) => secrets::read_plain(&path),
    }
}
