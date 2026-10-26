//! Direct flake inputs whose locked revision changed between two `flake.lock`
//! contents, named for display (plugin inputs `plugin<N>` by their source).
use serde_json::Value;

use crate::commands::web::plugins::plugin_label;

/// One locked revision of an input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedRev {
    /// Short git revision, or a short narHash for inputs without one (path:).
    pub rev: String,
    /// `lastModified` as `YYYY-MM-DD`.
    pub date: Option<String>,
}

/// One updated (added / removed) direct input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputUpdate {
    /// Display name (`neo`, `nixpkgs`, plugin repository name).
    pub name: String,
    pub old: Option<LockedRev>,
    pub new: Option<LockedRev>,
}

fn root_inputs(doc: &Value) -> Vec<String> {
    doc.get("root")
        .and_then(Value::as_str)
        .and_then(|r| doc.get("nodes")?.get(r)?.get("inputs")?.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

/// Lock node of a direct input (`follows` entries are arrays and yield `None`).
fn input_node<'a>(doc: &'a Value, input: &str) -> Option<&'a Value> {
    let nodes = doc.get("nodes")?;
    let root = doc.get("root")?.as_str()?;
    let node = nodes.get(root)?.get("inputs")?.get(input)?.as_str()?;
    nodes.get(node)
}

fn locked<'a>(doc: &'a Value, input: &str) -> Option<&'a Value> {
    input_node(doc, input)?.get("locked")
}

/// Plugin inputs are keyed `plugin<N>` in the generated flake; name them
/// after the plugin's source (`original` in the lock) instead.
fn plugin_input_label(doc: &Value, input: &str) -> Option<String> {
    let index = input.strip_prefix("plugin")?;
    if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let original = input_node(doc, input)?.get("original")?;
    let label = match original.get("repo").and_then(Value::as_str) {
        Some(repo) => plugin_label(repo),
        None => plugin_label(
            original
                .get("url")
                .or_else(|| original.get("path"))?
                .as_str()?,
        ),
    };
    (!label.is_empty()).then_some(label)
}

/// `YYYY-MM-DD` (UTC) of a unix timestamp.
fn ymd(secs: i64) -> String {
    // Howard Hinnant's civil_from_days.
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn locked_rev(locked: &Value) -> LockedRev {
    let rev = locked
        .get("rev")
        .and_then(Value::as_str)
        .map(|r| r.chars().take(7).collect())
        .or_else(|| {
            locked
                .get("narHash")
                .and_then(Value::as_str)
                .map(|h| h.trim_start_matches("sha256-").chars().take(7).collect())
        })
        .unwrap_or_else(|| "?".to_string());
    LockedRev {
        rev,
        date: locked.get("lastModified").and_then(Value::as_i64).map(ymd),
    }
}

/// Inputs whose `locked` entry differs, sorted by name. A lock change that
/// touches no direct input (only transitive nodes) is reported as `flake.lock`.
pub fn input_updates(old: &str, new: &str) -> Vec<InputUpdate> {
    if old == new {
        return Vec::new();
    }
    let (Ok(old), Ok(new)) = (
        serde_json::from_str::<Value>(old),
        serde_json::from_str::<Value>(new),
    ) else {
        return Vec::new();
    };
    let mut names = root_inputs(&new);
    names.extend(root_inputs(&old));
    names.sort();
    names.dedup();
    let mut out: Vec<InputUpdate> = names
        .into_iter()
        .filter_map(|n| {
            let (o, nw) = (locked(&old, &n), locked(&new, &n));
            if o == nw {
                return None;
            }
            let name = plugin_input_label(&new, &n)
                .or_else(|| plugin_input_label(&old, &n))
                .unwrap_or(n);
            Some(InputUpdate {
                name,
                old: o.map(locked_rev),
                new: nw.map(locked_rev),
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    if out.is_empty() && old.get("nodes") != new.get("nodes") {
        out.push(InputUpdate {
            name: "flake.lock".to_string(),
            old: None,
            new: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(old: &str, new: &str) -> Vec<String> {
        input_updates(old, new)
            .into_iter()
            .map(|u| u.name)
            .collect()
    }

    fn lock(nixpkgs: &str, neo: &str, transitive: &str) -> String {
        format!(
            r#"{{"root":"root","version":7,"nodes":{{
  "root":{{"inputs":{{"nixpkgs":"nixpkgs","neo":"neo","follows":["neo","nixpkgs"]}}}},
  "nixpkgs":{{"locked":{{"rev":"{nixpkgs}","narHash":"h-{nixpkgs}"}}}},
  "neo":{{"inputs":{{"systems":"systems"}},"locked":{{"rev":"{neo}","narHash":"h-{neo}"}}}},
  "systems":{{"locked":{{"rev":"{transitive}"}}}}
}}}}"#
        )
    }

    #[test]
    fn flake_lock_updates() {
        let base = lock("a", "b", "c");
        assert!(input_updates(&base, &base).is_empty());
        assert_eq!(names(&base, &lock("a2", "b", "c")), vec!["nixpkgs"]);
        assert_eq!(names(&base, &lock("a2", "b2", "c")), vec!["neo", "nixpkgs"]);
        assert_eq!(names(&base, &lock("a", "b", "c2")), vec!["flake.lock"]);
        // Reformatted JSON with the same content is no update.
        let pretty =
            serde_json::to_string_pretty(&serde_json::from_str::<Value>(&base).unwrap()).unwrap();
        assert!(input_updates(&base, &pretty).is_empty());
    }

    fn plugin_lock(rev: &str, original: &str) -> String {
        format!(
            r#"{{"root":"root","version":7,"nodes":{{
  "root":{{"inputs":{{"plugin0":"plugin0","plugin1":"plugin1"}}}},
  "plugin0":{{"locked":{{"rev":"{rev}","lastModified":1761436800}},"original":{original}}},
  "plugin1":{{"locked":{{"narHash":"sha256-abcdefghij"}},"original":{{"type":"path","path":"/opt/plugins/portrait"}}}}
}}}}"#
        )
    }

    #[test]
    fn plugin_inputs_use_source_name() {
        let github = r#"{"type":"github","owner":"me","repo":"high_sea","ref":"prod"}"#;
        let updates = input_updates(
            &plugin_lock("aaaaaaaaaa", github),
            &plugin_lock("bbbbbbbbbb", github),
        );
        assert_eq!(
            updates,
            vec![InputUpdate {
                name: "high_sea".into(),
                old: Some(LockedRev {
                    rev: "aaaaaaa".into(),
                    date: Some("2025-10-26".into())
                }),
                new: Some(LockedRev {
                    rev: "bbbbbbb".into(),
                    date: Some("2025-10-26".into())
                }),
            }]
        );
        let git = r#"{"type":"git","url":"https://example.org/me/portal.git","ref":"prod"}"#;
        assert_eq!(
            names(&plugin_lock("a", git), &plugin_lock("b", git)),
            vec!["portal"]
        );
    }

    #[test]
    fn ymd_formats_utc_dates() {
        assert_eq!(ymd(0), "1970-01-01");
        assert_eq!(ymd(951_782_400), "2000-02-29");
    }
}
