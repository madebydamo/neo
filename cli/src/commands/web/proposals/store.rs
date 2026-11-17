//! The proposal store: in memory, mirrored to a state file so cards survive the
//! neo-web restart of Save & apply. Holds no secret values.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use std::sync::{LazyLock, Mutex};

use super::types::{Proposal, Status};
use crate::commands::web::util::{state_dir, state_file};
use crate::utils::command::now_secs;

/// Proposals expire after a day (seconds).
pub(super) const TTL: u64 = 24 * 3600;

/// File in the neo-web state directory.
pub(super) const FILE: &str = "proposals.json";

pub(super) fn live(p: &Proposal) -> bool {
    now_secs().saturating_sub(p.created) < TTL
}

pub(super) fn store_path() -> Option<PathBuf> {
    // Tests use the in-memory store only, never the developer's state file.
    if cfg!(test) {
        return None;
    }
    state_dir().map(|d| d.join(FILE))
}

/// Proposals from the state file, expired ones dropped. Unreadable → none
/// (the store then lives in memory only).
pub(super) fn load_store() -> HashMap<String, Proposal> {
    store_path().map(|p| load_from(&p)).unwrap_or_default()
}

fn load_from(path: &Path) -> HashMap<String, Proposal> {
    let list: Vec<Proposal> = state_file::load_json(path).unwrap_or_default();
    list.into_iter()
        .filter(live)
        .map(|mut p| {
            interrupted(&mut p);
            (p.id.clone(), p)
        })
        .collect()
}

/// A proposal still `Saving` in the state file was cut off by a neo-web
/// restart mid-save: whether its write landed is unknown. It becomes `Failed`
/// (actionable again, so it never sticks) with a hint to check first, because
/// a retry runs the save again (e.g. generates a new secret).
pub(super) fn interrupted(p: &mut Proposal) {
    if p.status == Status::Saving {
        p.status = Status::Failed;
        p.message =
            "Saving was interrupted by a restart. Check the settings before trying again.".into();
    }
}

pub(super) fn write_store(store: &HashMap<String, Proposal>) -> anyhow::Result<()> {
    let path = store_path().context("no state directory for proposals")?;
    write_to(&path, store)
}

fn write_to(path: &Path, store: &HashMap<String, Proposal>) -> anyhow::Result<()> {
    let list: Vec<&Proposal> = store.values().collect();
    let body = serde_json::to_string(&list).context("serialize proposals")?;
    state_file::write_atomic(path, &body, 0o600)
}

pub(super) fn persist(store: &HashMap<String, Proposal>) {
    if let Err(e) = write_store(store) {
        eprintln!("web: proposals kept in memory only: {e:#}");
    }
}

pub(super) static STORE: LazyLock<Mutex<HashMap<String, Proposal>>> =
    LazyLock::new(|| Mutex::new(load_store()));

pub(super) fn new_id() -> String {
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

pub(super) fn put(p: Proposal) {
    if let Ok(mut store) = STORE.lock() {
        store.retain(|_, p| live(p));
        store.insert(p.id.clone(), p);
        persist(&store);
    }
}

/// Take an actionable proposal for saving: checks and marks it `Saving` in one
/// critical section, so of two concurrent clicks only one gets it (the other
/// gets `None` and shows the card as it is). The caller sets the final status
/// with [`set_status`], also on failure (back to an actionable one).
pub fn claim(id: &str) -> Option<Proposal> {
    let mut store = STORE.lock().ok()?;
    store.retain(|_, p| live(p));
    let p = store.get_mut(id).filter(|p| p.status.is_actionable())?;
    let before = p.clone();
    p.status = Status::Saving;
    p.message = "Saving…".into();
    persist(&store);
    Some(before)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::fsutil::TempDir;

    fn proposal(id: &str, status: Status, created: u64) -> Proposal {
        Proposal {
            id: id.into(),
            summary: "s".into(),
            views: vec![],
            asks: vec![],
            planned: vec![],
            fingerprint: "f".into(),
            status,
            message: String::new(),
            created,
        }
    }

    #[test]
    fn tests_never_touch_the_state_file() {
        assert_eq!(store_path(), None);
        assert!(write_store(&HashMap::new()).is_err());
    }

    #[test]
    fn expired_proposals_are_gone() {
        let now = now_secs();
        assert!(live(&proposal("a", Status::Pending, now - TTL + 60)));
        assert!(!live(&proposal("a", Status::Pending, now - TTL)));
        put(proposal("expired000000000", Status::Pending, now - TTL - 1));
        assert!(get("expired000000000").is_none());
        assert!(claim("expired000000000").is_none());
        put(proposal("fresh00000000000", Status::Pending, now));
        assert!(get("fresh00000000000").is_some());
    }

    #[test]
    fn state_file_round_trip() {
        let dir = TempDir::new("neo-proposals-test").unwrap();
        let path = dir.join(FILE);
        let now = now_secs();
        let store: HashMap<_, _> = [
            proposal("kept000000000000", Status::Pending, now),
            proposal("saving0000000000", Status::Saving, now),
            proposal("old0000000000000", Status::Pending, now - TTL - 1),
        ]
        .into_iter()
        .map(|p| (p.id.clone(), p))
        .collect();
        write_to(&path, &store).unwrap();
        let back = load_from(&path);
        // Expired ones are dropped, a save cut off by the restart can be retried.
        assert_eq!(back.len(), 2);
        let kept = &back["kept000000000000"];
        assert_eq!(
            (kept.status.clone(), kept.fingerprint.as_str()),
            (Status::Pending, "f")
        );
        assert_eq!(back["saving0000000000"].status, Status::Failed);
        // Missing or unreadable: an empty store.
        assert!(load_from(&dir.join("missing.json")).is_empty());
        std::fs::write(&path, "not json").unwrap();
        assert!(load_from(&path).is_empty());
    }
}
