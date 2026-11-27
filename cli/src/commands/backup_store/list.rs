//! `neo backup-store list [--json]`: what the store holds, who can restore it
//! and how to reach the store over this server's normal SSH
//! (docs/design/backup-store-ssh.md §3). Runs as any user: store.json and
//! report.json are world-readable and hold no secret; the host key is the
//! public half.
//!
//! `neo restore --from-store <login>` runs `ssh <login> neo backup-store list
//! --json` and reads [`StoreListing`].

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

use super::{
    ago, format_utc, now, report::key_info, size, ClientReport, Report, StoreConfig, REPORT_FILE,
};

/// What `list --json` prints.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreListing {
    /// The store server's hostname.
    pub host: String,
    /// The store's HTTPS address (`storeUrl` on the clients).
    pub url: String,
    /// The store's sshd on 127.0.0.1.
    pub ssh_port: u16,
    /// Its host key: `ssh-ed25519 AAAA…`, without the comment.
    pub host_key: String,
    /// When report.json was written (None: not yet).
    pub generated: Option<i64>,
    /// Total bytes stored across all clients in this store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_size_bytes: Option<u64>,
    /// Total capacity of the store's dataset/filesystem, in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_total_bytes: Option<u64>,
    /// Bytes available on the store's dataset/filesystem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_available_bytes: Option<u64>,
    pub clients: Vec<ClientReport>,
}

impl StoreListing {
    pub fn client(&self, name: &str) -> Option<&ClientReport> {
        self.clients.iter().find(|c| c.name == name)
    }
}

pub fn run(json: bool) -> Result<()> {
    let store = StoreConfig::load()?;
    let report = Report::load().ok();
    let host_key = fs::read_to_string(&store.host_key_file)
        .with_context(|| format!("read the store's host key {}", store.host_key_file))?;
    let host = fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|h| h.trim().to_string())
        .unwrap_or_default();
    let listing = build(&store, report, &host_key, &host);
    if json {
        println!("{}", serde_json::to_string_pretty(&listing)?);
    } else {
        print!("{}", text(&listing, now()));
    }
    Ok(())
}

fn build(store: &StoreConfig, report: Option<Report>, host_key: &str, host: &str) -> StoreListing {
    let (generated, total_size_bytes, disk_total_bytes, disk_available_bytes, clients) =
        match report {
            Some(r) => {
                let total = r.total_size_bytes();
                (
                    Some(r.generated),
                    total,
                    r.store.disk_total_bytes,
                    r.store.disk_available_bytes,
                    r.clients,
                )
            }
            // Before the first report: the clients of the settings, without data.
            None => (
                None,
                None,
                None,
                None,
                store
                    .clients
                    .iter()
                    .map(|(name, c)| ClientReport {
                        name: name.clone(),
                        format: c.format.clone(),
                        configured: true,
                        keys: c.public_keys.iter().filter_map(|k| key_info(k)).collect(),
                        ..Default::default()
                    })
                    .collect(),
            ),
        };
    StoreListing {
        host: host.to_string(),
        url: store.url.clone(),
        ssh_port: store.ssh_port,
        host_key: host_key
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" "),
        generated,
        total_size_bytes,
        disk_total_bytes,
        disk_available_bytes,
        clients,
    }
}

/// One line per fact, for people.
pub fn text(l: &StoreListing, t: i64) -> String {
    let mut out = format!("Backup store on {} ({})\n", l.host, l.url);
    out.push_str(&format!(
        "Store SSH: 127.0.0.1:{}, host key {}\n",
        l.ssh_port,
        short_key(&l.host_key)
    ));
    if let Some(total) = l.total_size_bytes {
        out.push_str(&format!("Total stored: {}", size(total)));
        if let (Some(avail), Some(disk)) = (l.disk_available_bytes, l.disk_total_bytes) {
            out.push_str(&format!(" ({} free of {})", size(avail), size(disk)));
        }
        out.push('\n');
    }
    if l.generated.is_none() {
        out.push_str(&format!(
            "! No report yet ({REPORT_FILE}): sizes and times come after the first run of backup-store-report.\n"
        ));
    }
    out.push('\n');
    let width = l.clients.iter().map(|c| c.name.len()).max().unwrap_or(0);
    for c in &l.clients {
        out.push_str(&client_line(c, width, t));
        out.push('\n');
        let pad = " ".repeat(width + 2);
        if !c.folders.is_empty() {
            out.push_str(&format!("{pad}folders: {}\n", c.folders.join(", ")));
        }
        out.push_str(&format!("{pad}can restore: {}\n", restorers(c)));
    }
    out
}

fn client_line(c: &ClientReport, width: usize, t: i64) -> String {
    let mut parts = vec![
        format!("{:width$}", c.name),
        format!("{:9}", if c.format.is_empty() { "?" } else { &c.format }),
    ];
    if let Some(v) = c.versions {
        parts.push(format!("{v} version{}", if v == 1 { "" } else { "s" }));
    }
    if let Some(b) = c.size_bytes {
        parts.push(size(b));
    }
    parts.push(match c.last_backup {
        Some(b) => format!(
            "last backup {} UTC ({}{})",
            format_utc(b),
            ago(t - b),
            if c.estimated { ", estimated" } else { "" }
        ),
        None => "no backup yet".into(),
    });
    if !c.configured {
        parts.push("(removed from the settings)".into());
    }
    parts.join("  ")
}

/// Who can restore a client: the keys that open it.
fn restorers(c: &ClientReport) -> String {
    let fmt = |k: &super::KeyInfo| format!("{} ({})", k.label, k.fingerprint);
    if c.keys.iter().any(|k| !k.role.is_empty()) {
        let unlock: Vec<String> = c
            .keys
            .iter()
            .filter(|k| k.role == "unlock")
            .map(fmt)
            .collect();
        if unlock.is_empty() {
            return format!(
                "{} host key only — add your key to System → SSH on {}; it counts after its next backup",
                c.name, c.name
            );
        }
        return unlock.join(", ");
    }
    if c.keys.is_empty() {
        return format!(
            "no key — add yours to the SSH public keys of {} here",
            c.name
        );
    }
    format!(
        "{} (the server's own key does not count: it is lost with the server)",
        c.keys.iter().map(fmt).collect::<Vec<_>>().join(", ")
    )
}

/// `ssh-ed25519 AAAAC3…xyz1` (the start and the end of the key).
fn short_key(line: &str) -> String {
    let mut w = line.split_whitespace();
    match (w.next(), w.next()) {
        (Some(t), Some(k)) if k.len() > 24 => {
            format!("{t} {}…{}", &k[..12], &k[k.len() - 8..])
        }
        _ => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::KeyInfo;
    use super::*;

    fn store() -> StoreConfig {
        serde_json::from_str(
            r#"{"url":"https://backup.example.com","path":"/var/backup-store","storage":"folder","internet":true,"login":true,"gate":"/g","sshPort":18023,"hostKeyFile":"/etc/ssh/ssh_host_ed25519_key.pub","clients":{"meilen":{"format":"encrypted","gatePrefix":"-"}}}"#,
        )
        .unwrap()
    }

    fn sample() -> StoreListing {
        let report: Report = serde_json::from_str(
            r#"{"generated":1795054503,"store":{"url":"https://backup.example.com","internet":true,"login":true},
               "clients":[
                 {"name":"meilen","format":"encrypted","configured":true,"estimated":false,"lastBackup":1795036503,"versions":14,"sizeBytes":182000000000,
                  "folders":["backup/meilen"],"keys":[{"label":"root@meilen","fingerprint":"SHA256:srv","role":"server"},{"label":"me@example.com","fingerprint":"SHA256:me","role":"unlock"}]},
                 {"name":"gossau","format":"zfs","configured":true,"estimated":false,"versions":30,"folders":[],"keys":[]}
               ]}"#,
        )
        .unwrap();
        build(
            &store(),
            Some(report),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHostKeyHostKeyHostKey root@brugg\n",
            "brugg",
        )
    }

    #[test]
    fn json_round_trip_holds_the_way_in() {
        let l = sample();
        assert_eq!(l.ssh_port, 18023);
        assert_eq!(
            l.host_key,
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHostKeyHostKeyHostKey"
        );
        let back: StoreListing = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
        assert_eq!(back.client("meilen").unwrap().versions, Some(14));
        assert_eq!(back.url, "https://backup.example.com");
    }

    #[test]
    fn text_names_who_can_restore() {
        let t = text(&sample(), 1_795_054_503);
        assert!(
            t.starts_with("Backup store on brugg (https://backup.example.com)"),
            "{t}"
        );
        assert!(t.contains("127.0.0.1:18023"), "{t}");
        assert!(t.contains("Total stored: 182 GB"), "{t}");
        assert!(t.contains("14 versions"), "{t}");
        assert!(t.contains("182 GB"), "{t}");
        assert!(t.contains("(5 hours ago)"), "{t}");
        assert!(t.contains("folders: backup/meilen"), "{t}");
        assert!(t.contains("can restore: me@example.com (SHA256:me)"), "{t}");
        assert!(!t.contains("root@meilen"), "the host key is not a way: {t}");
        assert!(t.contains("no key — add yours"), "{t}");
    }

    #[test]
    fn without_a_report_the_settings_list_the_clients() {
        let l = build(&store(), None, "ssh-ed25519 AAAA", "brugg");
        assert_eq!(l.clients.len(), 1);
        assert_eq!(l.clients[0].format, "encrypted");
        assert!(text(&l, 0).contains("No report yet"));
        let c = ClientReport {
            name: "x".into(),
            keys: vec![KeyInfo {
                label: "a".into(),
                fingerprint: "SHA256:a".into(),
                role: "access".into(),
            }],
            ..Default::default()
        };
        assert!(restorers(&c).contains("host key only"));
    }
}
