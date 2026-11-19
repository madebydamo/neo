//! `neo backup-store`: the store's rescue portal on the store's machine
//! (nix/services/backup-store, docs/design/backup-store-rescue.md).
//!
//! - `report`: what the store holds per client → [`REPORT_FILE`] (root, timer).
//! - `rescue`: restore a client onto a new machine from here (root, started by
//!   the web UI through systemd-run).
//!
//! Both read [`STORE_FILE`], which Nix writes from the store's settings.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub mod report;
pub mod rescue;

/// The store's settings as Nix renders them (backup-store/default.nix).
pub const STORE_FILE: &str = "/etc/neo/backup-store/store.json";
/// Written by `neo backup-store report`, read by the portal.
pub const REPORT_FILE: &str = "/var/lib/neo-backup-store/report.json";
/// Second AuthorizedKeysFile folder of the store's sshd: a rescue's short-lived key.
pub const RESCUE_KEYS_DIR: &str = "/run/neo-backup-store/rescue";

/// A client with a backup newer than this is "current" (green).
pub const CURRENT_SECS: i64 = 36 * 3600;
/// Older than this, or never: red.
pub const STALE_SECS: i64 = 7 * 24 * 3600;
/// A connection this recent means the old server may still run.
pub const RECENT_CONNECTION_SECS: i64 = 3600;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreClient {
    pub format: String,
    #[serde(default)]
    pub public_keys: Vec<String>,
    /// Second argument of the gate: the client's dataset, or "-".
    pub gate_prefix: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreConfig {
    /// https://<subdomain>.<domain>
    pub url: String,
    pub path: String,
    /// dataset | folder
    pub storage: String,
    #[serde(default)]
    pub dataset: String,
    /// ingress includes `web`: a new machine on the internet reaches the store.
    pub internet: bool,
    /// The portal asks for a login (tinyauth).
    pub login: bool,
    /// The forced command of the store's accounts (takes --read-only first).
    pub gate: String,
    #[serde(default)]
    pub clients: BTreeMap<String, StoreClient>,
}

impl StoreConfig {
    pub fn load() -> Result<Self> {
        Self::from_file(Path::new(STORE_FILE))
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("read {} (is the backup store on?)", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
    }

    pub fn home(&self, client: &str) -> PathBuf {
        Path::new(&self.path).join(client)
    }

    /// The client's dataset (dataset storage).
    pub fn client_dataset(&self, client: &str) -> Option<String> {
        (self.storage == "dataset").then(|| format!("{}/{client}", self.dataset))
    }

    /// `https://<client>@<host>/<folder>`: the --from-backup of a client's backup
    /// folder ("" or "." = the home folder).
    pub fn backup_url(&self, client: &str, folder: &str) -> String {
        let authority = self
            .url
            .trim_start_matches("https://")
            .trim_end_matches('/');
        let folder = match folder.trim_matches('/') {
            "." => "",
            f => f,
        };
        format!("https://{client}@{authority}/{folder}")
    }
}

/// One allowed key, for people to compare with theirs.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KeyInfo {
    /// The key's comment, or its type when it has none.
    pub label: String,
    /// `SHA256:…` as `ssh-keygen -l` prints it.
    pub fingerprint: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientReport {
    pub name: String,
    /// files | encrypted | zfs | "" (a removed client whose format is unknown)
    pub format: String,
    /// Still in the settings (false: its data stayed after removal).
    pub configured: bool,
    pub keys: Vec<KeyInfo>,
    /// Unix time of the newest backup.
    pub last_backup: Option<i64>,
    /// last_backup comes from file times, not from neo-backup.last or a snapshot.
    pub estimated: bool,
    pub versions: Option<u64>,
    pub size_bytes: Option<u64>,
    /// Last SSH login of the client's account (journal of backup-store-sshd).
    pub last_connection: Option<i64>,
    /// Backup folders in the home ("." = the home itself), files/encrypted.
    pub folders: Vec<String>,
    /// The config repo's `origin`, when the store can read it (files).
    pub git_remote: Option<String>,
    /// Hostname from neo-backup.last.
    pub host: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreSummary {
    pub url: String,
    pub internet: bool,
    pub login: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Unix time the report was written.
    pub generated: i64,
    pub store: StoreSummary,
    pub clients: Vec<ClientReport>,
}

impl Report {
    pub fn load() -> Result<Self> {
        let text =
            fs::read_to_string(REPORT_FILE).with_context(|| format!("read {REPORT_FILE}"))?;
        serde_json::from_str(&text).with_context(|| format!("parse {REPORT_FILE}"))
    }

    pub fn client(&self, name: &str) -> Option<&ClientReport> {
        self.clients.iter().find(|c| c.name == name)
    }
}

/// Client names: what the store's settings allow (also keeps paths sane).
pub fn client_name_ok(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b.len() <= 24
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `YYYY-MM-DDTHH:MM:SSZ` → Unix time.
pub fn parse_utc(s: &str) -> Option<i64> {
    let s = s.trim().strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':').map(|p| p.parse::<i64>().ok());
    let (h, min, sec) = (t.next()??, t.next()??, t.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) || h > 23 || min > 59 || sec > 60 {
        return None;
    }
    Some(days_from_civil(y, m, day) * 86400 + h * 3600 + min * 60 + sec)
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn run(action: BackupStoreAction) -> Result<()> {
    match action {
        BackupStoreAction::Report { out } => report::run(&out),
        BackupStoreAction::Rescue(args) => rescue::run(&args),
    }
}

#[derive(clap::Subcommand, Debug)]
pub enum BackupStoreAction {
    /// Collect what the store holds per client into the portal's report (root).
    Report {
        #[arg(long, default_value = REPORT_FILE)]
        out: PathBuf,
    },
    /// Restore a client onto a new machine from this store (root; the web
    /// portal starts it).
    Rescue(rescue::RescueArgs),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_times() {
        assert_eq!(parse_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_utc("2026-11-19T02:15:03Z"), Some(1_795_054_503));
        assert_eq!(parse_utc("2000-02-29T12:00:00Z"), Some(951_825_600));
        assert_eq!(parse_utc("2026-11-19 02:15:03"), None);
        assert_eq!(parse_utc("2026-13-01T00:00:00Z"), None);
    }

    #[test]
    fn client_names() {
        assert!(client_name_ok("brugg"));
        assert!(client_name_ok("dev-2"));
        for bad in ["", "-x", "Brugg", "a/b", "..", "a b", &"x".repeat(25)] {
            assert!(!client_name_ok(bad), "{bad}");
        }
    }

    #[test]
    fn backup_urls() {
        let s: StoreConfig = serde_json::from_str(
            r#"{"url":"https://backup.example.com","path":"/var/backup-store","storage":"folder","internet":true,"login":true,"gate":"/g"}"#,
        )
        .unwrap();
        assert_eq!(
            s.backup_url("brugg", "backups"),
            "https://brugg@backup.example.com/backups"
        );
        assert_eq!(
            s.backup_url("brugg", "."),
            "https://brugg@backup.example.com/"
        );
        assert_eq!(s.client_dataset("brugg"), None);
    }
}
