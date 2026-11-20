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
use super::{now, parse_utc, ClientReport, KeyInfo, Report, StoreConfig, StoreSummary};
use crate::utils::fsutil::write_atomic;
use crate::utils::profile::DEFAULT_CONFIG_REL;

/// The marker backup.sh writes after a good run to a store.
pub const MARKER: &str = "neo-backup.last";
const HEADER: &str = "neo-backup.age";
/// Basename of the backed-up folder in a files backup (`/var/neo` → `neo`).
const FILES_FOLDER: &str = "neo";
/// Folders searched below a home for backup folders.
const MAX_DEPTH: usize = 3;

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
    Report {
        generated: now(),
        store: StoreSummary {
            url: store.url.clone(),
            internet: store.internet,
            login: store.login,
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
                let (last, versions) = zfs_backups(&ds);
                r.last_backup = last;
                r.versions = versions;
            }
        }
        "files" | "encrypted" => {
            r.folders = backup_folders(&home, format);
            for folder in &r.folders {
                let dir = home.join(folder);
                let (last, estimated, host) = match read_marker(&dir) {
                    Some((t, h)) => (Some(t), false, h),
                    None => (estimate(&dir, format), true, None),
                };
                if last > r.last_backup {
                    r.last_backup = last;
                    r.estimated = estimated;
                    r.host = host.or(r.host.take());
                }
                if format == "encrypted" {
                    r.versions =
                        Some(r.versions.unwrap_or(0) + restic_snapshots(&dir).len() as u64);
                } else {
                    r.versions = Some(1);
                    if r.git_remote.is_none() {
                        r.git_remote = files_git_remote(&dir);
                    }
                }
            }
        }
        _ => {}
    }
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

/// Without a marker (clients from before it): newest snapshot file of the
/// restic repositories, or the time of the files backup's folder.
fn estimate(dir: &Path, format: &str) -> Option<i64> {
    if format == "encrypted" {
        restic_snapshots(dir).into_iter().max()
    } else {
        mtime(&dir.join(FILES_FOLDER))
    }
}

/// Modification times of the snapshot files of every restic-* repository.
fn restic_snapshots(dir: &Path) -> Vec<i64> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("restic-"))
        .filter_map(|e| fs::read_dir(e.path().join("snapshots")).ok())
        .flat_map(|snaps| snaps.flatten().filter_map(|s| mtime(&s.path())))
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

/// Newest `@neo-backup-*` snapshot below the client's dataset, and how many
/// the most recent backup dataset holds.
fn zfs_backups(dataset: &str) -> (Option<i64>, Option<u64>) {
    let Some(out) = output(Command::new("zfs").args([
        "list",
        "-H",
        "-p",
        "-r",
        "-t",
        "snapshot",
        "-o",
        "name,creation",
        dataset,
    ])) else {
        return (None, None);
    };
    let mut per_dataset: BTreeMap<String, (i64, u64)> = BTreeMap::new();
    for line in out.lines() {
        let Some((name, created)) = line.split_once('\t') else {
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
        let e = per_dataset.entry(ds.to_string()).or_insert((0, 0));
        e.0 = e.0.max(created);
        e.1 += 1;
    }
    match per_dataset.values().max_by_key(|(t, _)| *t) {
        Some((t, n)) => (Some(*t), Some(*n)),
        None => (None, Some(0)),
    }
}

fn size(store: &StoreConfig, name: &str) -> Option<u64> {
    if let Some(ds) = store.client_dataset(name) {
        let used = output(Command::new("zfs").args(["list", "-H", "-p", "-o", "used", &ds]))?;
        return used.trim().parse().ok();
    }
    let out = output(Command::new("du").arg("-sb").arg(store.home(name)))?;
    out.split_whitespace().next()?.parse().ok()
}

/// Comment and fingerprint of an authorized key line.
fn key_info(line: &str) -> Option<KeyInfo> {
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
        fs::write(home.join("enc/restic-1/snapshots/a"), "x").unwrap();
        fs::write(home.join("enc/restic-1/snapshots/b"), "x").unwrap();
        assert_eq!(backup_folders(home, "files"), ["backups"]);
        assert_eq!(backup_folders(home, "encrypted"), ["enc"]);
        assert_eq!(restic_snapshots(&home.join("enc")).len(), 2);
        fs::write(home.join(HEADER), "x").unwrap();
        assert_eq!(backup_folders(home, "encrypted"), ["."]);
    }
}
