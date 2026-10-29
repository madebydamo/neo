//! One-line "what changed" per config commit: services enabled / disabled /
//! reconfigured, other top-level sections touched and flake inputs updated,
//! from `settings.toml`, the secrets files and `flake.lock` at the commit and at
//! its first parent. Blobs are read with one `git cat-file --batch` call per
//! file for the whole window.
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use toml_edit::{DocumentMut, Item};

use crate::commands::web::diff::flake_lock::input_updates;
use crate::commands::web::diff::secrets_overlay::{secret_pair, SecretPair, SecretTexts};
use crate::commands::web::diff::settings_semantic::{diff_settings, Transition};
use crate::utils::{secrets, secrets_store};

/// Cached secret comparisons (value-free tokens) — at most this many.
const SECRET_CACHE_MAX: usize = 512;

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

/// Compare settings at a commit (`new`) with its parent (`old`); `secrets`
/// overlays changed secrets (secrets.yaml / secrets.sops.yaml) on their options.
pub fn summarize(old: Option<&str>, new: &str, secrets: &SecretPair) -> Option<ChangeSummary> {
    let enabled = enabled_count(new)?;
    let Some(old) = old else {
        return Some(ChangeSummary {
            initial: true,
            enabled,
            ..Default::default()
        });
    };
    let diff = diff_settings(old, new, &secrets.old, &secrets.new).ok()?;
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

/// Secret comparison of two versions; decrypting results are cached, since
/// commits never change and each decrypt goes through sudo + sops.
fn secrets_between(old: SecretTexts, new: SecretTexts) -> SecretPair {
    if old.plain == new.plain && old.sops == new.sops {
        return SecretPair::default();
    }
    if old.sops == new.sops {
        return secret_pair(old, new, &secrets_store::decrypt_text);
    }
    static CACHE: OnceLock<Mutex<HashMap<u64, SecretPair>>> = OnceLock::new();
    let mut h = DefaultHasher::new();
    for t in [old, new] {
        (t.settings, t.plain, t.sops).hash(&mut h);
    }
    let key = h.finish();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return hit;
    }
    let pair = secret_pair(old, new, &secrets_store::decrypt_text);
    // A failed decrypt (sudo / key) may work next time: not cached.
    if pair.note.is_none() {
        if let Ok(mut c) = cache.lock() {
            if c.len() >= SECRET_CACHE_MAX {
                c.clear();
            }
            c.insert(key, pair.clone());
        }
    }
    pair
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
    let plains = read_blobs(dir, &revs, secrets::PLAIN_FILE);
    let sopses = read_blobs(dir, &revs, secrets::SOPS_FILE);
    let locks = read_blobs(dir, &revs, "flake.lock");
    let texts = |rev: &str| SecretTexts {
        settings: blobs.get(rev).map_or("", String::as_str),
        plain: plains.get(rev).map_or("", String::as_str),
        sops: sopses.get(rev).map_or("", String::as_str),
    };
    commits
        .iter()
        .filter_map(|(c, p)| {
            let new = blobs.get(c)?;
            let old = match p {
                Some(p) => Some(blobs.get(p).map(String::as_str).unwrap_or("")),
                None => None,
            };
            let secrets = match p {
                Some(p) => secrets_between(texts(p), texts(c)),
                None => SecretPair::default(),
            };
            let mut s = summarize(old, new, &secrets)?;
            if let (Some(p), Some(new_lock)) = (p, locks.get(c)) {
                if let Some(old_lock) = locks.get(p) {
                    s.inputs = input_updates(old_lock, new_lock)
                        .into_iter()
                        .map(|u| u.name)
                        .collect();
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
        let s = summarize(None, BASE, &SecretPair::default()).unwrap();
        assert!(s.initial);
        assert_eq!(s.enabled, 2);
        let s = summarize(Some(BASE), BASE, &SecretPair::default()).unwrap();
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
        let s = summarize(Some(BASE), new, &SecretPair::default()).unwrap();
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
        let s = summarize(Some(BASE), new, &SecretPair::default()).unwrap();
        assert!(s.unchanged, "{s:?}");
    }

    #[test]
    fn new_service_table_disabled_is_not_added() {
        let new = format!("{BASE}\n[services.vaultwarden]\nenabled = false\n");
        let s = summarize(Some(BASE), &new, &SecretPair::default()).unwrap();
        assert!(s.added.is_empty());
        assert!(s.unchanged);
    }

    #[test]
    fn secret_only_change_marks_service_and_section() {
        let old = SecretTexts {
            settings: BASE,
            plain: "services:\n  jellyfin:\n    apiKey: \"a\"\ncore:\n  git:\n    tokens:\n      x: \"t\"\n",
            sops: "",
        };
        let new = SecretTexts {
            plain: "services:\n  jellyfin:\n    apiKey: \"b\"\ncore:\n  git:\n    tokens:\n      x: \"t\"\n",
            ..old
        };
        let s = summarize(Some(BASE), BASE, &secrets_between(old, new)).unwrap();
        assert_eq!(s.changed, vec!["jellyfin"]);
        assert!(s.sections.is_empty());
        assert!(!s.unchanged);
        assert!(secrets_between(old, old) == SecretPair::default());
    }

    #[test]
    fn unparsable_settings_give_no_summary() {
        assert!(summarize(Some(BASE), "not = [toml", &SecretPair::default()).is_none());
    }
}
