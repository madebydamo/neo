//! Secrets inside option values, by the option type: redaction for everything
//! the assistant or a proposal card shows, and checks that no secret value
//! travels through the assistant. A secret may sit at the top (`secret` on the
//! option) or deep inside (`accounts.<name>.password`, a submodule field with
//! `lib.neo.mkSecretOption`); `OptionType.fields` carry `secret` per field.

use serde_json::{json, Value};

use crate::commands::web::types::{OptionSchema, OptionType};
use crate::utils::secrets::is_placeholder;

/// Shown in place of a secret value.
pub const SET: &str = "<set>";
pub const UNSET: &str = "<unset>";

/// Whether a secret has a value (an encrypted placeholder counts as set).
pub fn is_set(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) => !m.is_empty(),
        _ => true,
    }
}

/// A submodule field by name. Plain nested submodules come flattened from
/// extract (`oauth.clientId`), so a key may also open a group of fields.
pub enum Field<'a> {
    One(&'a OptionSchema),
    Group(OptionType),
    Unknown,
}

pub fn field<'a>(fields: &'a [OptionSchema], name: &str) -> Field<'a> {
    if let Some(f) = fields.iter().find(|f| f.name == name) {
        return Field::One(f);
    }
    let prefix = format!("{name}.");
    let sub: Vec<OptionSchema> = fields
        .iter()
        .filter_map(|f| {
            f.name.strip_prefix(&prefix).map(|rest| OptionSchema {
                name: rest.to_string(),
                ..f.clone()
            })
        })
        .collect();
    if sub.is_empty() {
        Field::Unknown
    } else {
        Field::Group(OptionType {
            kind: "submodule".into(),
            fields: Some(sub),
            ..Default::default()
        })
    }
}

/// `v` with every secret position replaced by `<set>` / `<unset>` (and stray
/// encrypted placeholders by `<set>`). Unknown submodule fields are kept.
pub fn redact_value(t: &OptionType, secret: bool, v: &Value) -> Value {
    if secret {
        return json!(if is_set(v) { SET } else { UNSET });
    }
    if let Value::String(s) = v {
        if is_placeholder(s) {
            return json!(SET);
        }
    }
    let elem = t.elem.as_deref();
    match (t.kind.as_str(), v) {
        ("nullOr", _) => elem.map_or_else(|| v.clone(), |e| redact_value(e, false, v)),
        ("attrsOf", Value::Object(m)) => Value::Object(
            m.iter()
                .map(|(k, x)| {
                    let r = elem.map_or_else(|| redact_plain(x), |e| redact_value(e, false, x));
                    (k.clone(), r)
                })
                .collect(),
        ),
        ("listOf", Value::Array(a)) => Value::Array(
            a.iter()
                .map(|x| elem.map_or_else(|| redact_plain(x), |e| redact_value(e, false, x)))
                .collect(),
        ),
        ("submodule", Value::Object(m)) => {
            let fields = t.fields.as_deref().unwrap_or_default();
            Value::Object(
                m.iter()
                    .map(|(k, x)| {
                        let r = match field(fields, k) {
                            Field::One(f) => redact_value(&f.r#type, f.secret, x),
                            Field::Group(g) => redact_value(&g, false, x),
                            Field::Unknown => redact_plain(x),
                        };
                        (k.clone(), r)
                    })
                    .collect(),
            )
        }
        _ => redact_plain(v),
    }
}

/// Without a schema: only encrypted placeholders are known to be secrets.
fn redact_plain(v: &Value) -> Value {
    match v {
        Value::String(s) if is_placeholder(s) => json!(SET),
        Value::Array(a) => Value::Array(a.iter().map(redact_plain).collect()),
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, x)| (k.clone(), redact_plain(x)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Current value of an option, redacted.
pub fn redacted_current(o: &OptionSchema) -> Value {
    redact_value(
        &o.r#type,
        o.secret,
        o.current.as_ref().unwrap_or(&Value::Null),
    )
}

/// Default value of an option, redacted (a secret's default is never shown).
pub fn redacted_default(o: &OptionSchema) -> Value {
    if o.secret {
        Value::Null
    } else {
        redact_value(&o.r#type, false, &o.default)
    }
}

fn join(at: &str, k: &str) -> String {
    if at.is_empty() {
        k.to_string()
    } else {
        format!("{at}.{k}")
    }
}

/// Dotted position (relative to `v`, "" for `v` itself) of the first secret
/// that `v` gives a value to.
pub fn secret_in(t: &OptionType, secret: bool, v: &Value) -> Option<String> {
    fn rec(t: &OptionType, secret: bool, v: &Value, at: &str) -> Option<String> {
        if secret {
            return (!v.is_null()).then(|| at.to_string());
        }
        let elem = t.elem.as_deref();
        match (t.kind.as_str(), v) {
            ("nullOr", _) => elem.and_then(|e| rec(e, false, v, at)),
            ("attrsOf", Value::Object(m)) => {
                let e = elem?;
                m.iter().find_map(|(k, x)| rec(e, false, x, &join(at, k)))
            }
            ("listOf", Value::Array(a)) => {
                let e = elem?;
                a.iter()
                    .enumerate()
                    .find_map(|(i, x)| rec(e, false, x, &join(at, &(i + 1).to_string())))
            }
            ("submodule", Value::Object(m)) => {
                let fields = t.fields.as_deref().unwrap_or_default();
                m.iter().find_map(|(k, x)| match field(fields, k) {
                    Field::One(f) => rec(&f.r#type, f.secret, x, &join(at, k)),
                    Field::Group(g) => rec(&g, false, x, &join(at, k)),
                    Field::Unknown => None,
                })
            }
            _ => None,
        }
    }
    rec(t, secret, v, "")
}

/// Whether the type has a secret anywhere inside (or is one).
pub fn has_secret(t: &OptionType, secret: bool) -> bool {
    secret
        || t.elem.as_deref().is_some_and(|e| has_secret(e, false))
        || t.fields
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|f| has_secret(&f.r#type, f.secret))
}

/// Secrets that are set in `current` but missing from `new` (dotted, relative):
/// writing `new` over `current` would drop them. Entries of an attrsOf that
/// `new` leaves out are not counted (removing an entry is the caller's call).
pub fn dropped_secrets(t: &OptionType, secret: bool, current: &Value, new: &Value) -> Vec<String> {
    fn rec(
        t: &OptionType,
        secret: bool,
        cur: &Value,
        new: &Value,
        at: &str,
        out: &mut Vec<String>,
    ) {
        if secret {
            if is_set(cur) && new.is_null() {
                out.push(at.to_string());
            }
            return;
        }
        let elem = t.elem.as_deref();
        match (t.kind.as_str(), cur, new) {
            ("nullOr", _, _) => {
                if let Some(e) = elem {
                    rec(e, false, cur, new, at, out);
                }
            }
            ("attrsOf", Value::Object(c), Value::Object(n)) => {
                if let Some(e) = elem {
                    for (k, x) in n {
                        if let Some(old) = c.get(k) {
                            rec(e, false, old, x, &join(at, k), out);
                        }
                    }
                }
            }
            ("submodule", Value::Object(c), _) => {
                let fields = t.fields.as_deref().unwrap_or_default();
                for (k, old) in c {
                    let x = new.get(k).unwrap_or(&Value::Null);
                    match field(fields, k) {
                        Field::One(f) => rec(&f.r#type, f.secret, old, x, &join(at, k), out),
                        Field::Group(g) => rec(&g, false, old, x, &join(at, k), out),
                        Field::Unknown => {}
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    rec(t, secret, current, new, "", &mut out);
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// `accounts`: attrsOf submodule { username; password (secret); totpSecret (secret); notes }.
    pub fn accounts() -> OptionSchema {
        serde_json::from_value(json!({
            "name": "accounts",
            "label": "Accounts",
            "typeLabel": "attribute set of (submodule)",
            "type": {"kind": "attrsOf", "elem": {"kind": "submodule", "fields": [
                {"name": "username", "label": "Username", "typeLabel": "string", "type": {"kind": "str"}},
                {"name": "password", "label": "Password", "typeLabel": "null or string", "secret": true,
                 "type": {"kind": "nullOr", "elem": {"kind": "str"}},
                 "helper": {"kind": "button", "label": "Generate", "script": "/nix/store/x-gen"}},
                {"name": "totpSecret", "label": "TOTP secret", "typeLabel": "null or string", "secret": true,
                 "type": {"kind": "nullOr", "elem": {"kind": "str"}}},
                {"name": "notes", "label": "Notes", "typeLabel": "string", "type": {"kind": "str"}},
                {"name": "oauth.clientId", "label": "Client id", "typeLabel": "string", "type": {"kind": "str"}},
                {"name": "oauth.clientSecret", "label": "Client secret", "typeLabel": "string", "secret": true,
                 "type": {"kind": "str"}}
            ]}},
            "current": {"tinyauth": {
                "username": "admin",
                "password": "thispasswordisfortestingonly",
                "totpSecret": null,
                "notes": "",
                "oauth": {"clientId": "id", "clientSecret": "@NEO_SECRET:services/x/accounts/tinyauth/oauth/clientSecret@"}
            }}
        }))
        .unwrap()
    }

    #[test]
    fn nested_secrets_are_redacted() {
        let o = accounts();
        let r = redacted_current(&o);
        assert_eq!(
            r,
            json!({"tinyauth": {
                "username": "admin",
                "password": "<set>",
                "totpSecret": "<unset>",
                "notes": "",
                "oauth": {"clientId": "id", "clientSecret": "<set>"}
            }})
        );
        assert!(!r.to_string().contains("thispassword"));
        // Top-level secret and stray placeholders.
        let t: OptionType = serde_json::from_value(json!({"kind": "str"})).unwrap();
        assert_eq!(redact_value(&t, true, &json!("x")), json!("<set>"));
        assert_eq!(redact_value(&t, true, &json!("")), json!("<unset>"));
        assert_eq!(
            redact_value(&t, false, &json!("@NEO_SECRET:a/b@")),
            json!("<set>")
        );
        let any: OptionType = serde_json::from_value(json!({"kind": "unspecified"})).unwrap();
        assert_eq!(
            redact_value(&any, false, &json!({"a": ["@NEO_SECRET:a/b@", "ok"]})),
            json!({"a": ["<set>", "ok"]})
        );
    }

    #[test]
    fn secret_positions_in_values() {
        let o = accounts();
        let v = json!({"tmr": {"username": "tmr", "password": "pw"}});
        assert_eq!(
            secret_in(&o.r#type, false, &v).as_deref(),
            Some("tmr.password")
        );
        assert_eq!(
            secret_in(
                &o.r#type,
                false,
                &json!({"tmr": {"username": "tmr", "password": null}})
            ),
            None
        );
        assert_eq!(
            secret_in(
                &o.r#type,
                false,
                &json!({"t": {"oauth": {"clientSecret": "s"}}})
            )
            .as_deref(),
            Some("t.oauth.clientSecret")
        );
        assert!(has_secret(&o.r#type, false));
        let s: OptionType =
            serde_json::from_value(json!({"kind": "listOf", "elem": {"kind": "str"}})).unwrap();
        assert!(!has_secret(&s, false));
    }

    #[test]
    fn dropped_secrets_of_kept_entries() {
        let o = accounts();
        let cur = o.current.clone().unwrap();
        // Rewriting tinyauth without its password drops it (and the client secret).
        let mut d = dropped_secrets(
            &o.r#type,
            false,
            &cur,
            &json!({"tinyauth": {"username": "admin"}}),
        );
        d.sort();
        assert_eq!(d, vec!["tinyauth.oauth.clientSecret", "tinyauth.password"]);
        // Leaving tinyauth out entirely is a removal, not counted here.
        assert!(dropped_secrets(&o.r#type, false, &cur, &json!({"tmr": {}})).is_empty());
    }
}
