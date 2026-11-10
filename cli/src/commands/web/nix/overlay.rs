//! Lay settings saved since an evaluation over its results, so a page served
//! from that evaluation shows what was just saved (docs/design/eval-latency.md §5).
//!
//! Only values that differ between the two settings trees are touched; anything
//! the evaluation derived from them (defaults that depend on other settings,
//! units of a newly enabled service) is corrected when the next evaluation lands.
use serde_json::{Map, Value};

use super::extract::value_to_display;
use crate::commands::web::types::{IndexContext, OptionPaneContext, OptionSchema};

/// Sections under `neo.core.*`; the others are `neo.<section>`
/// (`coreSections` in extract_service_options.nix).
const CORE_SECTIONS: [&str; 8] = [
    "ssh",
    "volumes",
    "timeZone",
    "uid",
    "gid",
    "hostname",
    "hashedLinuxPassword",
    "plugins",
];

/// Settings path of a pane: `services.<name>`, `core.<section>` or `<section>`.
pub fn pane_prefix(is_core: bool, name: &str) -> Vec<String> {
    if !is_core {
        vec!["services".into(), name.into()]
    } else if CORE_SECTIONS.contains(&name) {
        vec!["core".into(), name.into()]
    } else {
        vec![name.into()]
    }
}

fn get<'a>(v: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter().try_fold(v, |v, k| v.get(k))
}

/// Settings path of one option of the pane at `prefix`. A scalar core section
/// (`timeZone`) is one option renamed to the section itself.
fn option_path(prefix: &[String], is_core: bool, name: &str) -> Vec<String> {
    let mut path = prefix.to_vec();
    if name.is_empty() || (is_core && prefix.last().map(String::as_str) == Some(name)) {
        return path;
    }
    path.extend(name.split('.').map(str::to_string));
    path
}

/// `cur` with the keys that changed from `old` to `new` applied; keys removed
/// from the settings drop out (the evaluation would fall back to the default).
fn apply_delta(cur: &mut Map<String, Value>, old: &Map<String, Value>, new: &Map<String, Value>) {
    for (k, nv) in new {
        let ov = old.get(k);
        if ov == Some(nv) {
            continue;
        }
        match (ov, cur.get_mut(k), nv) {
            (Some(Value::Object(o)), Some(Value::Object(c)), Value::Object(n)) => {
                apply_delta(c, o, n)
            }
            _ => {
                cur.insert(k.clone(), nv.clone());
            }
        }
    }
    for k in old.keys() {
        if !new.contains_key(k) {
            cur.remove(k);
        }
    }
}

/// New `current` of `o`, or `None` when its settings did not change.
fn patched_current(o: &OptionSchema, old: Option<&Value>, new: Option<&Value>) -> Option<Value> {
    if old == new {
        return None;
    }
    Some(match (old, new, o.current.as_ref()) {
        (_, None, _) => o.default.clone(),
        (Some(Value::Object(ov)), Some(Value::Object(nv)), Some(Value::Object(cur))) => {
            let mut cur = cur.clone();
            apply_delta(&mut cur, ov, nv);
            Value::Object(cur)
        }
        (_, Some(nv), _) => nv.clone(),
    })
}

/// Lay `new` settings over a pane evaluated from `old`. True when anything changed.
pub fn pane(pane: &mut OptionPaneContext, old: &Value, new: &Value) -> bool {
    let prefix = pane_prefix(pane.is_core, &pane.service);
    if get(old, &prefix) == get(new, &prefix) {
        return false;
    }
    let mut changed = false;
    for o in pane.options.iter_mut() {
        let path = option_path(&prefix, pane.is_core, &o.name);
        if let Some(cur) = patched_current(o, get(old, &path), get(new, &path)) {
            o.currentDisplay = value_to_display(&cur);
            o.current = Some(cur);
            changed = true;
        }
    }
    if !changed {
        return false;
    }
    for o in pane.sections.iter_mut().flat_map(|s| s.options.iter_mut()) {
        if let Some(src) = pane.options.iter().find(|p| p.name == o.name) {
            o.current = src.current.clone();
            o.currentDisplay = src.currentDisplay.clone();
        }
    }
    if let Ok(json) = serde_json::to_string(&pane.options) {
        pane.options_json = json;
    }
    true
}

/// Lay `new` settings over the services grid (the `enabled` flags).
pub fn services(idx: &mut IndexContext, old: &Value, new: &Value) -> bool {
    let mut changed = false;
    for group in idx.groups.iter_mut() {
        for s in group.services.iter_mut() {
            let path = [
                "services".to_string(),
                s.name.clone(),
                "enabled".to_string(),
            ];
            let (ov, nv) = (get(old, &path), get(new, &path));
            if ov != nv {
                s.enabled = nv.and_then(Value::as_bool).unwrap_or(false);
                changed = true;
            }
        }
        group.has_enabled = group.services.iter().any(|s| s.enabled);
        group.has_disabled = group.services.iter().any(|s| !s.enabled);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn opt(name: &str, default: Value, current: Value) -> OptionSchema {
        serde_json::from_value(json!({
            "name": name, "type": {"kind": "str"}, "typeLabel": "str",
            "default": default, "current": current
        }))
        .unwrap()
    }

    fn pane_of(service: &str, is_core: bool, options: Vec<OptionSchema>) -> OptionPaneContext {
        let mut p = crate::commands::web::nix::extract::error_pane_for(service, is_core);
        p.sections = vec![crate::commands::web::types::OptionSection {
            options: options.clone(),
            ..Default::default()
        }];
        p.options = options;
        p
    }

    #[test]
    fn changed_scalar_and_removed_key() {
        let mut p = pane_of(
            "immich",
            false,
            vec![
                opt("subdomain", json!("immich"), json!("photos")),
                opt("auth.enabled", json!(true), json!(false)),
                opt("port", json!(80), json!(80)),
            ],
        );
        let old =
            json!({"services": {"immich": {"subdomain": "photos", "auth": {"enabled": false}}}});
        let new = json!({"services": {"immich": {"subdomain": "pics", "port": 81}}});
        assert!(pane(&mut p, &old, &new));
        assert_eq!(p.options[0].current, Some(json!("pics")));
        assert_eq!(p.options[0].currentDisplay, "pics");
        // auth.enabled left settings.toml: back to its default.
        assert_eq!(p.options[1].current, Some(json!(true)));
        assert_eq!(p.options[2].current, Some(json!(81)));
        assert_eq!(p.sections[0].options[0].current, Some(json!("pics")));
        assert!(p.options_json.contains("\"pics\""));
    }

    #[test]
    fn attrs_entries_keep_evaluated_defaults() {
        let mut p = pane_of(
            "tinyauth",
            false,
            vec![opt(
                "accounts",
                json!({}),
                json!({"a": {"password": "x", "admin": false}, "b": {"password": "y", "admin": false}}),
            )],
        );
        let old = json!({"services": {"tinyauth": {"accounts": {"a": {"password": "x"}, "b": {"password": "y"}}}}});
        let new = json!({"services": {"tinyauth": {"accounts": {"a": {"password": "z"}, "c": {"password": "w"}}}}});
        assert!(pane(&mut p, &old, &new));
        assert_eq!(
            p.options[0].current,
            Some(json!({"a": {"password": "z", "admin": false}, "c": {"password": "w"}}))
        );
    }

    #[test]
    fn untouched_pane_and_core_paths() {
        let mut p = pane_of(
            "immich",
            false,
            vec![opt("subdomain", json!("immich"), json!("immich"))],
        );
        let old = json!({"services": {"other": {"enabled": true}}});
        let new = json!({"services": {"other": {"enabled": false}}});
        assert!(!pane(&mut p, &old, &new));

        let mut tz = pane_of(
            "timeZone",
            true,
            vec![opt("timeZone", json!("UTC"), json!("UTC"))],
        );
        let old = json!({});
        let new = json!({"core": {"timeZone": "Europe/Zurich"}});
        assert!(pane(&mut tz, &old, &new));
        assert_eq!(tz.options[0].current, Some(json!("Europe/Zurich")));

        assert_eq!(pane_prefix(true, "ssh"), ["core", "ssh"]);
        assert_eq!(pane_prefix(true, "nix"), ["nix"]);
        assert_eq!(pane_prefix(false, "a"), ["services", "a"]);
    }

    #[test]
    fn services_enabled_flags() {
        let mut idx: IndexContext = IndexContext {
            groups: serde_json::from_value(json!([{
                "name": "Media",
                "services": [
                    {"name": "a", "enabled": false, "category": "Media", "description": ""},
                    {"name": "b", "enabled": true, "category": "Media", "description": ""}
                ],
                "has_enabled": true, "has_disabled": true
            }]))
            .unwrap(),
            ..Default::default()
        };
        let old = json!({"services": {"b": {"enabled": true}}});
        let new = json!({"services": {"a": {"enabled": true}, "b": {"enabled": true}}});
        assert!(services(&mut idx, &old, &new));
        assert!(idx.groups[0].services[0].enabled);
        assert!(!idx.groups[0].has_disabled);
        assert!(!services(&mut idx, &new, &new));
    }
}
