//! User notes on settings versions (config-repo commits) and system
//! generations, e.g. "known good after setup".
//!
//! Stored as one small JSON file in neo-web's systemd `StateDirectory`
//! (`/var/lib/neo-web/version-notes.json`, on the root filesystem), not in the
//! config repo: git notes would travel with the repo and be rewound by a Neo
//! data restore (the repo lives on the restored dataset), and generations are
//! not commits at all. The root filesystem is never snapshotted or restored,
//! like the activation log the timeline already relies on.
//!
//! Generations are keyed by number plus link creation time, so a note does not
//! jump to a different system if the profile is ever reset and numbers repeat.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::model::NotesView;
use crate::commands::web::util::{state_dir, state_file};
use crate::commands::web::zfs::sanitize_comment;

const FILE: &str = "version-notes.json";

/// Serializes read-modify-write of the notes file.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GenNote {
    pub note: String,
    /// Creation time of `system-<n>-link` when the note was written.
    pub created: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct VersionNotes {
    #[serde(default)]
    pub commits: BTreeMap<String, String>,
    #[serde(default)]
    pub generations: BTreeMap<u64, GenNote>,
}

impl VersionNotes {
    /// Notes for the timeline: generation notes only while the link still has
    /// the creation time they were written for.
    pub fn view(&self, gen_created: &[(u64, i64)]) -> NotesView {
        let generations = self
            .generations
            .iter()
            .filter(|(n, g)| {
                gen_created
                    .iter()
                    .any(|(gn, c)| gn == *n && *c == g.created)
            })
            .map(|(n, g)| (*n, g.note.clone()))
            .collect();
        NotesView {
            commits: self.commits.clone(),
            generations,
        }
    }
}

fn notes_path() -> Option<PathBuf> {
    state_dir().map(|d| d.join(FILE))
}

/// Missing or unreadable file → no notes (never fails the timeline).
pub fn load() -> VersionNotes {
    notes_path()
        .and_then(|p| state_file::load_json(&p))
        .unwrap_or_default()
}

fn save(notes: &VersionNotes) -> Result<()> {
    let path = notes_path().context("no state directory for version notes")?;
    let body = serde_json::to_string_pretty(notes).context("serialize version notes")?;
    state_file::write_atomic(&path, &body, 0o644)
}

/// Full commit id (SHA-1 or SHA-256 hex).
pub fn commit_id_ok(id: &str) -> bool {
    (id.len() == 40 || id.len() == 64) && id.chars().all(|c| c.is_ascii_hexdigit())
}

/// What a note is attached to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoteTarget {
    Commit(String),
    /// Generation number and its link creation time.
    Generation(u64, i64),
}

/// Apply one change in memory; an empty (after sanitizing) note removes it.
pub fn apply(notes: &mut VersionNotes, target: &NoteTarget, note: &str) -> Result<()> {
    let note = sanitize_comment(note);
    match target {
        NoteTarget::Commit(id) => {
            if !commit_id_ok(id) {
                bail!("invalid commit id");
            }
            let id = id.to_ascii_lowercase();
            match note {
                Some(n) => notes.commits.insert(id, n),
                None => notes.commits.remove(&id),
            };
        }
        NoteTarget::Generation(n, created) => match note {
            Some(note) => {
                notes.generations.insert(
                    *n,
                    GenNote {
                        note,
                        created: *created,
                    },
                );
            }
            None => {
                notes.generations.remove(n);
            }
        },
    }
    Ok(())
}

/// Set or clear a note and persist it.
pub fn set(target: &NoteTarget, note: &str) -> Result<()> {
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("version notes lock poisoned"))?;
    let mut notes = load();
    apply(&mut notes, target, note)?;
    save(&notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn commit_ids() {
        assert!(commit_id_ok(SHA));
        assert!(!commit_id_ok("abcdef0"));
        assert!(!commit_id_ok("activation_20240101-120000"));
        assert!(!commit_id_ok(&"g".repeat(40)));
    }

    #[test]
    fn apply_sets_and_clears() {
        let mut n = VersionNotes::default();
        apply(
            &mut n,
            &NoteTarget::Commit(SHA.to_uppercase()),
            " known\ngood ",
        )
        .unwrap();
        assert_eq!(n.commits.get(SHA).map(String::as_str), Some("known good"));
        apply(&mut n, &NoteTarget::Generation(7, 100), "fresh setup").unwrap();
        assert_eq!(n.generations[&7].created, 100);
        apply(&mut n, &NoteTarget::Commit(SHA.into()), "  ").unwrap();
        assert!(n.commits.is_empty());
        apply(&mut n, &NoteTarget::Generation(7, 100), "").unwrap();
        assert!(n.generations.is_empty());
        assert!(apply(&mut n, &NoteTarget::Commit("HEAD".into()), "x").is_err());
    }

    #[test]
    fn view_drops_reused_generation_numbers() {
        let mut n = VersionNotes::default();
        apply(&mut n, &NoteTarget::Generation(7, 100), "good").unwrap();
        apply(&mut n, &NoteTarget::Generation(8, 200), "gone").unwrap();
        let v = n.view(&[(7, 100), (8, 999)]);
        assert_eq!(v.generations.get(&7).map(String::as_str), Some("good"));
        assert!(!v.generations.contains_key(&8));
    }

    #[test]
    fn file_format_round_trips() {
        let mut n = VersionNotes::default();
        apply(&mut n, &NoteTarget::Commit(SHA.into()), "a").unwrap();
        apply(&mut n, &NoteTarget::Generation(3, 5), "b").unwrap();
        let t = serde_json::to_string(&n).unwrap();
        let back: VersionNotes = serde_json::from_str(&t).unwrap();
        assert_eq!(back, n);
        let empty: VersionNotes = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, VersionNotes::default());
    }
}
