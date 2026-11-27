//! `neo backup-store report`: what the store holds per client, for the rescue
//! portal (docs/design/backup-store-rescue.md §2). Runs as root from
//! backup-store-report.service: the client homes are 0700 per account, and
//! `du` is too slow for a page request. First it updates the restore keys of
//! the encrypted clients ([`super::recipients`]).

use anyhow::Result;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use super::recipients::{self, Role, Unlock};
use super::{
    now, parse_rfc3339, parse_utc, BackupEntry, ClientReport, KeyInfo, Report, StoreConfig,
    StoreSummary,
};
use crate::utils::fsutil::write_atomic;
use crate::utils::profile::DEFAULT_CONFIG_REL;

/// The marker backup.sh writes after a good run to a store.
pub const MARKER: &str = "neo-backup.last";
/// Plain-text snapshot list backup.sh writes next to an encrypted backup
/// (short ID, timestamp, logical size, packed delta — no secrets or paths).
pub const SNAPSHOTS_FILE: &str = "neo-backup.snapshots";
const HEADER: &str = "neo-backup.age";
/// Basename of the backed-up folder in a files backup (`/var/neo` → `neo`).
const FILES_FOLDER: &str = "neo";
/// Folders searched below a home for backup folders.
const MAX_DEPTH: usize = 3;
/// Bound snapshot metadata lines read from an untrusted client file.
const MAX_SNAPSHOTS: usize = 2000;

pub fn run(out: &Path) -> Result<()> {
    let store = StoreConfig::load()?;
    // Before du: a key added on a client gets access within one run.
    let unlocks = restore_keys(&store)?;
    let report = build(&store, &last_connections(), &unlocks);
    let body = serde_json::to_vec_pretty(&report)?;
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    write_atomic(out, &body, Some(0o644))?;
    println!(
        "backup-store report: {} client(s) → {}",
        report.clients.len(),
        out.display()
    );
    Ok(())
}

/// What opens each encrypted client's backups, and the read-only keys file
/// of its account.
fn restore_keys(store: &StoreConfig) -> Result<BTreeMap<String, Unlock>> {
    let mut unlocks = BTreeMap::new();
    let mut files = BTreeMap::new();
    for (name, c) in &store.clients {
        if c.format != "encrypted" {
            continue;
        }
        let home = store.home(name);
        let u = recipients::collect(&home, &backup_folders(&home, "encrypted"));
        files.insert(
            name.clone(),
            recipients::keys_file(store, &c.format, &c.gate_prefix, &u, &c.public_keys),
        );
        unlocks.insert(name.clone(), u);
    }
    recipients::write_all(&files)?;
    Ok(unlocks)
}

/// The keys shown for a client: those of the settings and, for an encrypted
/// one, the admin keys its backup is encrypted to.
fn client_keys(public_keys: &[String], unlock: Option<&Unlock>) -> Vec<KeyInfo> {
    let mut keys: Vec<KeyInfo> = public_keys
        .iter()
        .filter_map(|line| {
            let mut k = key_info(line)?;
            if let Some(u) = unlock.filter(|u| u.known || !u.tags.is_empty()) {
                k.role = match u.role_of(line) {
                    Some(Role::Server) => "server",
                    Some(Role::Admin) => "unlock",
                    None => "access",
                }
                .into();
            }
            Some(k)
        })
        .collect();
    if let Some(u) = unlock {
        for r in &u.keys {
            let fingerprint = r.fingerprint();
            if keys.iter().any(|k| k.fingerprint == fingerprint) {
                continue;
            }
            keys.push(KeyInfo {
                label: r.label(),
                fingerprint,
                role: match r.role {
                    Role::Server => "server",
                    Role::Admin => "unlock",
                }
                .into(),
            });
        }
    }
    keys
}

fn build(
    store: &StoreConfig,
    connections: &BTreeMap<String, i64>,
    unlocks: &BTreeMap<String, Unlock>,
) -> Report {
    let mut clients: Vec<ClientReport> = store
        .clients
        .iter()
        .map(|(name, c)| {
            let mut r = client(store, name, &c.format);
            r.configured = true;
            r.keys = client_keys(&c.public_keys, unlocks.get(name));
            r.last_connection = connections.get(name).copied();
            r
        })
        .collect();
    // Homes of removed clients keep their data.
    if let Ok(entries) = fs::read_dir(&store.path) {
        let mut removed: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| super::client_name_ok(n) && !store.clients.contains_key(n))
            .collect();
        removed.sort();
        for name in removed {
            let format = guess_format(store, &name);
            let mut r = client(store, &name, &format);
            r.last_connection = connections.get(&name).copied();
            clients.push(r);
        }
    }
    let total_size_bytes = if clients.is_empty() {
        Some(0)
    } else if clients.iter().any(|c| c.size_bytes.is_some()) {
        Some(clients.iter().filter_map(|c| c.size_bytes).sum())
    } else {
        None
    };
    let (disk_total_bytes, disk_used_bytes, disk_available_bytes) = store_space(store);
    Report {
        generated: now(),
        store: StoreSummary {
            url: store.url.clone(),
            internet: store.internet,
            login: store.login,
            total_size_bytes,
            storage: store.storage.clone(),
            disk_total_bytes,
            disk_used_bytes,
            disk_available_bytes,
        },
        clients,
    }
}

fn client(store: &StoreConfig, name: &str, format: &str) -> ClientReport {
    let home = store.home(name);
    let mut r = ClientReport {
        name: name.to_string(),
        format: format.to_string(),
        size_bytes: size(store, name),
        ..Default::default()
    };
    match format {
        "zfs" => {
            if let Some(ds) = store.client_dataset(name) {
                let (last, versions, backups) = zfs_backups(&ds);
                r.last_backup = last;
                r.versions = versions;
                r.backups = backups;
            }
        }
        "files" | "encrypted" => {
            r.folders = backup_folders(&home, format);
            let single_home = r.folders.len() == 1 && r.folders[0] == ".";
            for folder in &r.folders {
                let dir = home.join(folder);
                if format == "encrypted" {
                    let entries = encrypted_backups(&dir, folder);
                    let newest = entries.first().map(|b| (b.time, b.estimated));
                    let (last, estimated, host) = match read_marker(&dir) {
                        Some((t, h)) => (Some(t), false, h),
                        None => match newest {
                            Some((t, est)) => (Some(t), est, None),
                            None => (None, true, None),
                        },
                    };
                    if last > r.last_backup {
                        r.last_backup = last;
                        r.estimated = estimated;
                        r.host = host.or(r.host.take());
                    }
                    r.versions = Some(r.versions.unwrap_or(0) + entries.len() as u64);
                    r.backups.extend(entries);
                } else {
                    let (last, estimated, host) = match read_marker(&dir) {
                        Some((t, h)) => (Some(t), false, h),
                        None => (mtime(&dir.join(FILES_FOLDER)), true, None),
                    };
                    if last > r.last_backup {
                        r.last_backup = last;
                        r.estimated = estimated;
                        r.host = host.or(r.host.take());
                    }
                    let folder_size = if single_home {
                        r.size_bytes
                    } else {
                        dir_size(&dir)
                    };
                    if let Some(time) = last {
                        r.backups.push(BackupEntry {
                            time,
                            id: if folder == "." {
                                "files".into()
                            } else {
                                folder.clone()
                            },
                            folder: folder.clone(),
                            size_bytes: folder_size,
                            delta_bytes: None,
                            estimated,
                        });
                    }
                    r.versions = Some(r.folders.len() as u64);
                    if r.git_remote.is_none() {
                        r.git_remote = files_git_remote(&dir);
                    }
                }
            }
        }
        _ => {}
    }
    r.backups
        .sort_by(|a, b| b.time.cmp(&a.time).then_with(|| b.id.cmp(&a.id)));
    r.oldest_backup = r.backups.iter().map(|b| b.time).min();
    r
}

/// A removed client's format, from what its home holds.
fn guess_format(store: &StoreConfig, name: &str) -> String {
    let home = store.home(name);
    if !backup_folders(&home, "encrypted").is_empty() {
        "encrypted".into()
    } else if !backup_folders(&home, "files").is_empty() {
        "files".into()
    } else if store
        .client_dataset(name)
        .is_some_and(|ds| zfs_backups(&ds).1.unwrap_or(0) > 0)
    {
        "zfs".into()
    } else {
        String::new()
    }
}

/// Folders below `home` that hold a backup of `format`, relative ("." = home).
fn backup_folders(home: &Path, format: &str) -> Vec<String> {
    let mut found = Vec::new();
    walk(home, home, 0, format, &mut found);
    found.sort();
    found
}

fn walk(home: &Path, dir: &Path, depth: usize, format: &str, found: &mut Vec<String>) {
    let is_backup = match format {
        "encrypted" => dir.join(HEADER).is_file(),
        _ => dir.join(FILES_FOLDER).join("DATA").is_dir(),
    };
    if is_backup {
        let rel = dir.strip_prefix(home).unwrap_or(dir).display().to_string();
        found.push(if rel.is_empty() { ".".into() } else { rel });
        return;
    }
    if depth >= MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        // Not into a repository or a backed-up tree.
        if name.starts_with("restic-") || name.starts_with('.') {
            continue;
        }
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            walk(home, &e.path(), depth + 1, format, found);
        }
    }
}

/// `time=` and `host=` of neo-backup.last.
fn read_marker(dir: &Path) -> Option<(i64, Option<String>)> {
    parse_marker(&fs::read_to_string(dir.join(MARKER)).ok()?)
}

fn parse_marker(text: &str) -> Option<(i64, Option<String>)> {
    let field = |k: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(k)?.strip_prefix('='))
            .map(str::trim)
    };
    let time = parse_utc(field("time")?)?;
    let host = field("host").filter(|h| !h.is_empty()).map(String::from);
    Some((time, host))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotMeta {
    time: i64,
    size_bytes: Option<u64>,
    delta_bytes: Option<u64>,
}

/// Snapshot entries of an encrypted backup folder: every file in
/// `restic-*/snapshots/`, enriched with exact time and sizes from
/// `neo-backup.snapshots` when the client uploaded it.
fn encrypted_backups(dir: &Path, folder: &str) -> Vec<BackupEntry> {
    let meta = fs::read_to_string(dir.join(SNAPSHOTS_FILE))
        .map(|text| parse_snapshots_file(&text))
        .unwrap_or_default();
    let mut entries: Vec<BackupEntry> = restic_snapshot_files(dir)
        .into_iter()
        .map(|(short_id, file_mtime)| {
            if let Some(m) = meta.get(&short_id) {
                BackupEntry {
                    time: m.time,
                    id: short_id,
                    folder: folder.to_string(),
                    size_bytes: m.size_bytes,
                    delta_bytes: m.delta_bytes,
                    estimated: false,
                }
            } else {
                BackupEntry {
                    time: file_mtime,
                    id: short_id,
                    folder: folder.to_string(),
                    size_bytes: None,
                    delta_bytes: None,
                    estimated: true,
                }
            }
        })
        .collect();
    entries.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| b.id.cmp(&a.id)));
    entries
}

/// Parse `neo-backup.snapshots`: `<short_id>\t<rfc3339>\t<size>\t<delta>`.
fn parse_snapshots_file(text: &str) -> BTreeMap<String, SnapshotMeta> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        if map.len() >= MAX_SNAPSHOTS {
            break;
        }
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = l.split_whitespace().collect();
        let (Some(&raw_id), Some(&raw_time)) = (parts.first(), parts.get(1)) else {
            continue;
        };
        if !(6..=64).contains(&raw_id.len()) || !raw_id.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let Some(time) = parse_rfc3339(raw_time) else {
            continue;
        };
        let opt_u64 = |idx: usize| -> Option<u64> {
            let s = *parts.get(idx)?;
            if s == "-" {
                None
            } else {
                s.parse().ok()
            }
        };
        let short_id = raw_id[..raw_id.len().min(8)].to_ascii_lowercase();
        map.insert(
            short_id,
            SnapshotMeta {
                time,
                size_bytes: opt_u64(2),
                delta_bytes: opt_u64(3),
            },
        );
    }
    map
}

/// `(short_id, mtime)` of every snapshot file across `restic-*` repositories.
fn restic_snapshot_files(dir: &Path) -> Vec<(String, i64)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("restic-"))
        .filter_map(|e| fs::read_dir(e.path().join("snapshots")).ok())
        .flat_map(|snaps| {
            snaps.flatten().filter_map(|s| {
                let name = s.file_name().to_string_lossy().into_owned();
                let time = mtime(&s.path())?;
                let short_id = name
                    .chars()
                    .take(8)
                    .collect::<String>()
                    .to_ascii_lowercase();
                Some((short_id, time))
            })
        })
        .collect()
}

fn mtime(p: &Path) -> Option<i64> {
    let t = fs::metadata(p).ok()?.modified().ok()?;
    t.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

/// The config repo's origin in a files backup (plain text in .git/config).
fn files_git_remote(dir: &Path) -> Option<String> {
    let config = dir
        .join(FILES_FOLDER)
        .join(DEFAULT_CONFIG_REL)
        .join(".git/config");
    origin_url(&fs::read_to_string(config).ok()?)
}

/// `url` of `[remote "origin"]` in a git config.
fn origin_url(git_config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in git_config.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_origin = l == r#"[remote "origin"]"#;
        } else if in_origin {
            if let Some((k, v)) = l.split_once('=') {
                if k.trim() == "url" {
                    return Some(v.trim().to_string()).filter(|u| !u.is_empty());
                }
            }
        }
    }
    None
}

/// All `@neo-backup-*` snapshots below the client's dataset, with creation
/// time, unique `used` delta and full `referenced` size.
fn zfs_backups(dataset: &str) -> (Option<i64>, Option<u64>, Vec<BackupEntry>) {
    let Some(out) = output(Command::new("zfs").args([
        "list",
        "-H",
        "-p",
        "-r",
        "-t",
        "snapshot",
        "-o",
        "name,creation,used,referenced",
        dataset,
    ])) else {
        return (None, None, Vec::new());
    };
    parse_zfs_backups(dataset, &out)
}

fn parse_zfs_backups(dataset: &str, out: &str) -> (Option<i64>, Option<u64>, Vec<BackupEntry>) {
    let prefix = format!("{dataset}/");
    let mut backups = Vec::new();
    for line in out.lines() {
        let mut cols = line.split('\t');
        let (Some(name), Some(created)) = (cols.next(), cols.next()) else {
            continue;
        };
        let Some((ds, snap)) = name.split_once('@') else {
            continue;
        };
        if !snap.starts_with(crate::utils::backup::restore::SNAPSHOT_PREFIX) {
            continue;
        }
        let Ok(created) = created.trim().parse::<i64>() else {
            continue;
        };
        let used = cols.next().and_then(|s| s.trim().parse::<u64>().ok());
        let referenced = cols.next().and_then(|s| s.trim().parse::<u64>().ok());
        let folder = ds
            .strip_prefix(&prefix)
            .map(String::from)
            .unwrap_or_default();
        backups.push(BackupEntry {
            time: created,
            id: snap.to_string(),
            folder,
            size_bytes: referenced,
            delta_bytes: used,
            estimated: false,
        });
    }
    backups.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| b.id.cmp(&a.id)));
    let last = backups.first().map(|b| b.time);
    let versions = Some(backups.len() as u64);
    (last, versions, backups)
}

fn size(store: &StoreConfig, name: &str) -> Option<u64> {
    if let Some(ds) = store.client_dataset(name) {
        let used = output(Command::new("zfs").args(["list", "-H", "-p", "-o", "used", &ds]))?;
        return used.trim().parse().ok();
    }
    dir_size(&store.home(name))
}

fn dir_size(path: &Path) -> Option<u64> {
    let out = output(Command::new("du").arg("-sb").arg(path))?;
    out.split_whitespace().next()?.parse().ok()
}

/// `(total, used, available)` bytes of the store's dataset or filesystem.
fn store_space(store: &StoreConfig) -> (Option<u64>, Option<u64>, Option<u64>) {
    if store.storage == "dataset" && !store.dataset.is_empty() {
        if let Some(out) = output(Command::new("zfs").args([
            "list",
            "-H",
            "-p",
            "-o",
            "used,available",
            &store.dataset,
        ])) {
            if let Some((total, used, avail)) = parse_zfs_space(&out) {
                return (Some(total), Some(used), Some(avail));
            }
        }
    }
    if let Some(out) =
        output(Command::new("df").args(["-B1", "--output=size,used,avail", &store.path]))
    {
        if let Some((total, used, avail)) = parse_df_space(&out) {
            return (Some(total), Some(used), Some(avail));
        }
    }
    (None, None, None)
}

fn parse_zfs_space(out: &str) -> Option<(u64, u64, u64)> {
    let line = out.lines().next()?.trim();
    let mut parts = line.split_whitespace();
    let used: u64 = parts.next()?.parse().ok()?;
    let avail: u64 = parts.next()?.parse().ok()?;
    Some((used.saturating_add(avail), used, avail))
}

fn parse_df_space(out: &str) -> Option<(u64, u64, u64)> {
    let line = out.lines().rev().find(|l| !l.trim().is_empty())?;
    let mut parts = line.split_whitespace();
    let total: u64 = parts.next()?.parse().ok()?;
    let used: u64 = parts.next()?.parse().ok()?;
    let avail: u64 = parts.next()?.parse().ok()?;
    Some((total, used, avail))
}

/// Comment and fingerprint of an authorized key line.
pub(crate) fn key_info(line: &str) -> Option<KeyInfo> {
    let mut child = Command::new("ssh-keygen")
        .args(["-l", "-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child
        .stdin
        .take()?
        .write_all(format!("{line}\n").as_bytes())
        .ok()?;
    let out = child.wait_with_output().ok()?;
    if !out.status.success() {
        return None;
    }
    let fingerprint = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)?
        .to_string();
    Some(KeyInfo {
        label: key_label(line),
        fingerprint,
        role: String::new(),
    })
}

/// The comment of `type base64 [comment]`, else the type.
fn key_label(line: &str) -> String {
    let parts: Vec<&str> = line.split_whitespace().collect();
    match parts.as_slice() {
        [_, _, comment @ ..] if !comment.is_empty() => comment.join(" "),
        [kind, ..] => kind.to_string(),
        [] => String::new(),
    }
}

/// Last "Accepted publickey for backup-<client>" per client in the store
/// sshd's journal (30 days).
fn last_connections() -> BTreeMap<String, i64> {
    let out = output(Command::new("journalctl").args([
        "-u",
        "backup-store-sshd",
        "--since=-30d",
        "-o",
        "short-unix",
        "--no-pager",
        "-q",
    ]))
    .unwrap_or_default();
    parse_connections(&out)
}

fn parse_connections(journal: &str) -> BTreeMap<String, i64> {
    let mut last = BTreeMap::new();
    for line in journal.lines() {
        let Some((_, rest)) = line.split_once("Accepted publickey for backup-") else {
            continue;
        };
        let Some(client) = rest.split_whitespace().next() else {
            continue;
        };
        let Some(t) = line
            .split_whitespace()
            .next()
            .and_then(|t| t.split('.').next())
            .and_then(|t| t.parse::<i64>().ok())
        else {
            continue;
        };
        last.insert(client.to_string(), t);
    }
    last
}

fn output(cmd: &mut Command) -> Option<String> {
    let out = cmd.stderr(Stdio::null()).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::fsutil::TempDir;

    #[test]
    fn marker_fields() {
        let (t, host) =
            parse_marker("version=1\ntime=1970-01-02T00:00:00Z\nhost=brugg\nformat=files\n")
                .unwrap();
        assert_eq!(t, 86400);
        assert_eq!(host.as_deref(), Some("brugg"));
        assert!(parse_marker("version=1\n").is_none());
    }

    #[test]
    fn connections_from_the_journal() {
        let j = "1795000000.123 meilen sshd[1]: Accepted publickey for backup-brugg from 127.0.0.1 port 1 ssh2: ED25519 SHA256:x\n\
                 1795000100.5 meilen sshd[2]: Accepted publickey for backup-brugg from 127.0.0.1 port 2 ssh2: ED25519 SHA256:x\n\
                 1795000050.0 meilen sshd[3]: Accepted publickey for backup-wohlen from 127.0.0.1 port 3 ssh2\n\
                 1795000060.0 meilen sshd[4]: Connection closed by 127.0.0.1\n";
        let c = parse_connections(j);
        assert_eq!(c.get("brugg"), Some(&1_795_000_100));
        assert_eq!(c.get("wohlen"), Some(&1_795_000_050));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn origin_of_a_git_config() {
        let c = "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = git@github.com:me/conf.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n[remote \"other\"]\n\turl = x\n";
        assert_eq!(origin_url(c).as_deref(), Some("git@github.com:me/conf.git"));
        assert_eq!(origin_url("[remote \"other\"]\n url = x\n"), None);
    }

    #[test]
    fn key_labels() {
        assert_eq!(key_label("ssh-ed25519 AAAA damian@laptop"), "damian@laptop");
        assert_eq!(key_label("ssh-ed25519 AAAA"), "ssh-ed25519");
        assert_eq!(key_label("ssh-ed25519 AAAA two words"), "two words");
    }

    #[test]
    fn finds_backup_folders() {
        let t = TempDir::new("neo-store-report").unwrap();
        let home = t.path();
        fs::create_dir_all(home.join("backups/neo/DATA")).unwrap();
        fs::create_dir_all(home.join("enc/restic-1/snapshots")).unwrap();
        fs::write(home.join("enc/neo-backup.age"), "x").unwrap();
        fs::write(home.join("enc/restic-1/snapshots/a1b2c3d400000000"), "x").unwrap();
        fs::write(home.join("enc/restic-1/snapshots/deadbeef11111111"), "x").unwrap();
        assert_eq!(backup_folders(home, "files"), ["backups"]);
        assert_eq!(backup_folders(home, "encrypted"), ["enc"]);
        assert_eq!(restic_snapshot_files(&home.join("enc")).len(), 2);

        // Snapshot metadata enriches matching files and ignores vanished IDs.
        fs::write(
            home.join("enc").join(SNAPSHOTS_FILE),
            "# neo-backup.snapshots version=1\n\
             a1b2c3d4\t2026-11-26T03:00:00Z\t50000000000\t120000000\n\
             ffffffff\t2026-11-20T03:00:00Z\t1000\t10\n",
        )
        .unwrap();
        let entries = encrypted_backups(&home.join("enc"), "enc");
        assert_eq!(entries.len(), 2);
        let enriched = entries.iter().find(|e| e.id == "a1b2c3d4").unwrap();
        assert_eq!(enriched.size_bytes, Some(50_000_000_000));
        assert_eq!(enriched.delta_bytes, Some(120_000_000));
        assert!(!enriched.estimated);
        let fallback = entries.iter().find(|e| e.id == "deadbeef").unwrap();
        assert_eq!(fallback.size_bytes, None);
        assert!(fallback.estimated);

        fs::write(home.join(HEADER), "x").unwrap();
        assert_eq!(backup_folders(home, "encrypted"), ["."]);
    }

    #[test]
    fn parses_zfs_backups_and_store_space() {
        let out = "\
tank/backup-store/gossau/neo@neo-backup-20261125-030000\t1795489200\t15000000\t48000000000\n\
tank/backup-store/gossau/neo@neo-backup-20261126-030000\t1795575600\t22000000\t48500000000\n\
tank/backup-store/gossau/neo@manual-keep\t1795600000\t100\t48500000000\n";
        let (last, versions, backups) = parse_zfs_backups("tank/backup-store/gossau", out);
        assert_eq!(last, Some(1_795_575_600));
        assert_eq!(versions, Some(2));
        assert_eq!(backups.len(), 2);
        assert_eq!(backups[0].id, "neo-backup-20261126-030000");
        assert_eq!(backups[0].folder, "neo");
        assert_eq!(backups[0].size_bytes, Some(48_500_000_000));
        assert_eq!(backups[0].delta_bytes, Some(22_000_000));
        assert_eq!(backups[1].id, "neo-backup-20261125-030000");

        assert_eq!(
            parse_zfs_space("400000000000\t1600000000000\n"),
            Some((2_000_000_000_000, 400_000_000_000, 1_600_000_000_000))
        );
        assert_eq!(
            parse_df_space("   1B-blocks         Used        Avail\n2000000000000 400000000000 1600000000000\n"),
            Some((2_000_000_000_000, 400_000_000_000, 1_600_000_000_000))
        );
    }
}
