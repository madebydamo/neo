//! The proposal store: in memory, mirrored to a state file so cards survive the
//! neo-web restart of Save & apply. Holds no secret values.
use super::*;

pub(super) fn live(p: &Proposal) -> bool {
    now_secs().saturating_sub(p.created) < TTL
}

pub(super) fn store_path() -> Option<PathBuf> {
    state_dir().map(|d| d.join(FILE))
}

/// Proposals from the state file, expired ones dropped. Unreadable → none
/// (the store then lives in memory only).
pub(super) fn load_store() -> HashMap<String, Proposal> {
    let list: Vec<Proposal> = store_path()
        .and_then(|p| state_file::load_json(&p))
        .unwrap_or_default();
    list.into_iter()
        .filter(live)
        .map(|p| (p.id.clone(), p))
        .collect()
}

pub(super) fn write_store(store: &HashMap<String, Proposal>) -> anyhow::Result<()> {
    use anyhow::Context;
    let path = store_path().context("no state directory for proposals")?;
    let list: Vec<&Proposal> = store.values().collect();
    let body = serde_json::to_string(&list).context("serialize proposals")?;
    state_file::write_atomic(&path, &body, 0o600)
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

pub fn set_status(id: &str, status: Status, message: &str) -> Option<Proposal> {
    let mut store = STORE.lock().ok()?;
    let p = store.get_mut(id)?;
    p.status = status;
    p.message = message.to_string();
    let p = p.clone();
    persist(&store);
    Some(p)
}
