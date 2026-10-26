//! One-line "what changed" per config commit: services enabled / disabled /
//! reconfigured, other top-level sections touched and flake inputs updated,
//! from `settings.toml` / `flake.lock` at the commit and at its first parent.
//! Blobs are read with one `git cat-file --batch` call per file for the whole
//! window.
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Serialize;
use toml_edit::{DocumentMut, Item};

use crate::commands::web::diff::settings_semantic::{diff_settings_toml, Transition};
use crate::commands::web::plugins::plugin_label;

#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSummary {
    /// First version (no parent to compare with).
    pub initial: bool,
    /// settings.toml identical to the parent (re-activation, other files).
    pub unchanged: bool,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// Enabled services whose settings changed.
    pub changed: Vec<String>,
    /// Other top-level sections that changed (e.g. `core`, `swag`).
    pub sections: Vec<String>,
    /// Flake inputs whose locked revision changed (`flake.lock` update).
    pub inputs: Vec<String>,
    /// Services enabled in this version.
    pub enabled: usize,
}

/// Direct flake inputs whose `locked` entry differs between two `flake.lock`
/// contents. A lock change that touches no direct input (only transitive
/// nodes) is reported as `flake.lock`.
pub fn updated_inputs(old: &str, new: &str) -> Vec<String> {
    use serde_json::Value;
    if old == new {
        return Vec::new();
    }
    let (Ok(old), Ok(new)) = (
        serde_json::from_str::<Value>(old),
        serde_json::from_str::<Value>(new),
    ) else {
        return Vec::new();
    };
    let locked = |doc: &Value, input: &str| -> Option<Value> {
        let nodes = doc.get("nodes")?;
        let root = doc.get("root")?.as_str()?;
        let node = nodes.get(root)?.get("inputs")?.get(input)?.as_str()?;
        nodes.get(node)?.get("locked").cloned()
    };
    let root_inputs = |doc: &Value| -> Vec<String> {
        doc.get("root")
            .and_then(Value::as_str)
            .and_then(|r| doc.get("nodes")?.get(r)?.get("inputs")?.as_object())
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    };
    let mut names = root_inputs(&new);
    names.extend(root_inputs(&old));
    names.sort();
    names.dedup();
    let mut out: Vec<String> = names
        .into_iter()
        .filter(|n| locked(&old, n) != locked(&new, n))
        .map(|n| {
            plugin_input_label(&new, &n)
                .or_else(|| plugin_input_label(&old, &n))
                .unwrap_or(n)
        })
        .collect();
    out.sort();
    out.dedup();
    if out.is_empty() && old.get("nodes") != new.get("nodes") {
        out.push("flake.lock".to_string());
    }
    out
}

/// Plugin inputs are keyed `plugin<N>` in the generated flake; name them
/// after the plugin's source (`original` in the lock) instead.
fn plugin_input_label(doc: &serde_json::Value, input: &str) -> Option<String> {
    let index = input.strip_prefix("plugin")?;
    if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let nodes = doc.get("nodes")?;
    let root = doc.get("root")?.as_str()?;
    let node = nodes.get(root)?.get("inputs")?.get(input)?.as_str()?;
    let original = nodes.get(node)?.get("original")?;
    let label = match original.get("repo").and_then(|r| r.as_str()) {
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

impl ChangeSummary {
    fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && self.sections.is_empty()
            && self.inputs.is_empty()
    }
}

fn enabled_count(raw: &str) -> Option<usize> {
    let doc: DocumentMut = raw.parse().ok()?;
    let services = doc.get("services").and_then(Item::as_table_like);
    Some(services.map_or(0, |t| {
        t.iter()
            .filter(|(_, svc)| {
                svc.as_table_like()
                    .and_then(|t| t.get("enabled"))
                    .and_then(Item::as_bool)
                    .unwrap_or(false)
            })
            .count()
    }))
}

/// Compare settings at a commit (`new`) with its parent (`old`).
pub fn summarize(old: Option<&str>, new: &str) -> Option<ChangeSummary> {
    let enabled = enabled_count(new)?;
    let Some(old) = old else {
        return Some(ChangeSummary {
            initial: true,
            enabled,
            ..Default::default()
        });
    };
    let diff = diff_settings_toml(old, new).ok()?;
    let mut s = ChangeSummary {
        enabled,
        ..Default::default()
    };
    for svc in diff.services {
        match svc.transition() {
            Transition::Enabled => s.added.push(svc.name),
            Transition::Disabled => s.removed.push(svc.name),
            Transition::Unchanged if svc.before == Some(true) && !svc.changes.is_empty() => {
                s.changed.push(svc.name)
            }
            Transition::Unchanged => {}
        }
    }
    s.sections = diff.sections.into_iter().map(|sec| sec.name).collect();
    s.unchanged = s.is_empty();
    Some(s)
}

/// `file` contents at each rev (missing file / rev → absent).
fn read_blobs(dir: &Path, revs: &[String], file: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if revs.is_empty() {
        return out;
    }
    let Ok(mut child) = Command::new("git")
        .current_dir(dir)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return out;
    };
    let input: String = revs.iter().map(|r| format!("{r}:{file}\n")).collect();
    // Write from a thread so a full stdout pipe cannot deadlock us.
    let writer = child.stdin.take().map(|mut stdin| {
        std::thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        })
    });
    if let Some(stdout) = child.stdout.take() {
        let mut r = BufReader::new(stdout);
        for rev in revs {
            let mut header = String::new();
            if r.read_line(&mut header).unwrap_or(0) == 0 {
                break;
            }
            // `<oid> blob <size>` or `<input> missing`
            let mut parts = header.split_whitespace();
            let (_, kind, size) = (parts.next(), parts.next(), parts.next());
            let Some(size) = size.and_then(|s| s.parse::<usize>().ok()) else {
                continue;
            };
            let mut buf = vec![0u8; size + 1]; // content + trailing LF
            if r.read_exact(&mut buf).is_err() {
                break;
            }
            if kind == Some("blob") {
                buf.truncate(size);
                out.insert(rev.clone(), String::from_utf8_lossy(&buf).into_owned());
            }
        }
    }
    if let Some(w) = writer {
        let _ = w.join();
    }
    let _ = child.wait();
    out
}

/// Summaries for `(commit, first parent)` pairs, keyed by commit.
pub fn summarize_commits(
    dir: &Path,
    commits: &[(String, Option<String>)],
) -> HashMap<String, ChangeSummary> {
    let mut revs: Vec<String> = Vec::new();
    for (c, p) in commits {
        revs.push(c.clone());
        if let Some(p) = p {
            revs.push(p.clone());
        }
    }
    revs.sort();
    revs.dedup();
    let blobs = read_blobs(dir, &revs, "settings.toml");
    let locks = read_blobs(dir, &revs, "flake.lock");
    commits
        .iter()
        .filter_map(|(c, p)| {
            let new = blobs.get(c)?;
            let old = match p {
                Some(p) => Some(blobs.get(p).map(String::as_str).unwrap_or("")),
                None => None,
            };
            let mut s = summarize(old, new)?;
            if let (Some(p), Some(new_lock)) = (p, locks.get(c)) {
                if let Some(old_lock) = locks.get(p) {
                    s.inputs = updated_inputs(old_lock, new_lock);
                    s.unchanged = s.is_empty();
                }
            }
            Some((c.clone(), s))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"
[core]
timezone = "UTC"

[services.jellyfin]
enabled = true
subdomain = "tv"

[services.immich]
enabled = true

[services.paperless]
enabled = false
"#;

    #[test]
    fn initial_and_unchanged() {
        let s = summarize(None, BASE).unwrap();
        assert!(s.initial);
        assert_eq!(s.enabled, 2);
        let s = summarize(Some(BASE), BASE).unwrap();
        assert!(s.unchanged);
    }

    #[test]
    fn enable_disable_and_reconfigure() {
        let new = r#"
[core]
timezone = "Europe/Zurich"

[services.jellyfin]
enabled = true
subdomain = "media"

[services.immich]
enabled = false

[services.paperless]
enabled = true
"#;
        let s = summarize(Some(BASE), new).unwrap();
        assert_eq!(s.added, vec!["paperless"]);
        assert_eq!(s.removed, vec!["immich"]);
        assert_eq!(s.changed, vec!["jellyfin"]);
        assert_eq!(s.sections, vec!["core"]);
        assert!(!s.unchanged && !s.initial);
        assert_eq!(s.enabled, 2);
    }

    #[test]
    fn formatting_only_is_unchanged() {
        let new = "# comment\n[services.immich]\nenabled = true\n\n[services.jellyfin]\nsubdomain = \"tv\"\nenabled = true\n[core]\ntimezone = \"UTC\"\n[services.paperless]\nenabled = false\n";
        let s = summarize(Some(BASE), new).unwrap();
        assert!(s.unchanged, "{s:?}");
    }

    #[test]
    fn new_service_table_disabled_is_not_added() {
        let new = format!("{BASE}\n[services.vaultwarden]\nenabled = false\n");
        let s = summarize(Some(BASE), &new).unwrap();
        assert!(s.added.is_empty());
        assert!(s.unchanged);
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
        assert!(updated_inputs(&base, &base).is_empty());
        assert_eq!(
            updated_inputs(&base, &lock("a2", "b", "c")),
            vec!["nixpkgs"]
        );
        assert_eq!(
            updated_inputs(&base, &lock("a2", "b2", "c")),
            vec!["neo", "nixpkgs"]
        );
        assert_eq!(
            updated_inputs(&base, &lock("a", "b", "c2")),
            vec!["flake.lock"]
        );
        // Reformatted JSON with the same content is no update.
        let pretty = serde_json::to_string_pretty(
            &serde_json::from_str::<serde_json::Value>(&base).unwrap(),
        )
        .unwrap();
        assert!(updated_inputs(&base, &pretty).is_empty());
    }

    fn plugin_lock(rev: &str, original: &str) -> String {
        format!(
            r#"{{"root":"root","version":7,"nodes":{{
  "root":{{"inputs":{{"plugin0":"plugin0","plugin1":"plugin1"}}}},
  "plugin0":{{"locked":{{"rev":"{rev}"}},"original":{original}}},
  "plugin1":{{"locked":{{"rev":"x"}},"original":{{"type":"path","path":"/opt/plugins/portrait"}}}}
}}}}"#
        )
    }

    #[test]
    fn plugin_inputs_use_source_name() {
        let github = r#"{"type":"github","owner":"me","repo":"high_sea","ref":"prod"}"#;
        assert_eq!(
            updated_inputs(&plugin_lock("a", github), &plugin_lock("b", github)),
            vec!["high_sea"]
        );
        let git = r#"{"type":"git","url":"https://example.org/me/portal.git","ref":"prod"}"#;
        assert_eq!(
            updated_inputs(&plugin_lock("a", git), &plugin_lock("b", git)),
            vec!["portal"]
        );
    }

    #[test]
    fn unparsable_settings_give_no_summary() {
        assert!(summarize(Some(BASE), "not = [toml").is_none());
    }
}
