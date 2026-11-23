//! `neo backup`: the encryption key of the backup service and reading a
//! backup from elsewhere (docs/design/backup-restore.md §3).

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

use crate::commands::restore::store;
use crate::utils::backup::config_repo::{commit_uncommitted, remote_url};
use crate::utils::backup::store_ssh;
use crate::utils::backup::{self, default_key, Access, Format, Source};
use crate::utils::command::execute_command;
use crate::utils::command::root_command;
use crate::utils::fsutil::TempDir;

const ROTATE_UNIT: &str = "backup-rotate-key.service";

/// Start backup-rotate-key.service: new data key, new repository, full backup,
/// old repository removed (backup.sh). It waits for a running backup.
pub fn rotate_key(doc: &DocumentMut, dry_run: bool, sudo_cmd: &str) -> Result<()> {
    // Every encrypted destination gets a new key. No sources when the backup is off.
    let encrypted = Source::from_settings(doc)?
        .iter()
        .any(|s| s.format == Format::Encrypted);
    if !encrypted {
        bail!("only the encrypted backup has a key: add a destination with format = \"encrypted\" and apply first");
    }
    if dry_run {
        println!("DRY-RUN: would start {ROTATE_UNIT}");
        return Ok(());
    }
    execute_command(root_command("systemctl", sudo_cmd, false).args([
        "start",
        "--no-block",
        ROTATE_UNIT,
    ]))
    .context("start the key rotation (is the encrypted backup applied on this machine?)")?;
    println!("Key rotation started: each encrypted destination gets a new key and a full backup with it, then its old backup is deleted.");
    println!("It runs in the background (journalctl -fu {ROTATE_UNIT}); the old backup stays usable until it is done.");
    Ok(())
}

/// `neo backup list --from-store <login>…`: what each store holds and which of
/// your keys opens what (docs/design/backup-store-ssh.md §4).
pub fn list(logins: &[String], key: Option<PathBuf>) -> Result<()> {
    let extra: Vec<PathBuf> = key.into_iter().collect();
    let mine = store_ssh::my_keys(&extra);
    for login in logins {
        let listing = store_ssh::fetch(login)?;
        store_ssh::show(&listing, &mine);
        println!("  Restore one: neo restore --from-store {login} --client <name> --host root@<new machine>");
        println!();
    }
    Ok(())
}

/// Where `extract-config` reads from.
pub enum ConfigFrom {
    /// Any `--from-backup` form.
    Backup(String),
    /// A store over SSH: the client given, or the one your keys open.
    Store {
        login: String,
        client: Option<String>,
    },
}

/// Copy the config repo out of a backup into `out`.
pub fn extract_config(
    from: &ConfigFrom,
    out: &Path,
    key: Option<PathBuf>,
    backup_key: Option<PathBuf>,
    dry_run: bool,
) -> Result<()> {
    let spec = match from {
        ConfigFrom::Backup(b) => b.clone(),
        ConfigFrom::Store { login, .. } => format!("--from-store {login}"),
    };
    let spec = spec.as_str();
    if dry_run {
        println!(
            "DRY-RUN: would copy the config repo from {spec} to {}",
            out.display()
        );
        return Ok(());
    }
    let key = key.or_else(default_key);
    let work = TempDir::new("neo-backup")?;
    let (source, store_key) = match from {
        ConfigFrom::Backup(b) => (store::backup_source(b, work.path())?, None),
        ConfigFrom::Store { login, client } => {
            let stores = store::open_stores(
                std::slice::from_ref(login),
                key.as_deref(),
                client.as_deref(),
                true,
                true,
            )?;
            let source = stores.routes[0]
                .picked_source(work.path())?
                .context("no backup picked")?;
            (source, stores.key)
        }
    };
    let access = Access {
        identities: backup_key.into_iter().chain(store_key).chain(key).collect(),
        known_hosts: work.join("known_hosts"),
        work: work.path().to_path_buf(),
    };
    let source = backup::detect(&access, source)?;
    println!("Reading the {} …", source.describe());
    let rel = backup::extract_config(&access, &source, out)?;
    commit_uncommitted(out)?;
    println!(
        "✓ Config repo ({rel} in the backup) is at {}",
        out.display()
    );
    match remote_url(out).as_str() {
        "" => println!("  It has no remote: push it somewhere safe (git remote add origin …)."),
        url => println!("  Its remote is {url}; push it if that copy is gone."),
    }
    if let ConfigFrom::Store { login, .. } = from {
        println!("  Restore a machine: neo restore --from-store {login} --host root@<new machine>");
    } else if spec.starts_with('/') || spec.starts_with('.') {
        println!(
            "  Restore a machine (as root on it, with the disk attached): neo restore {} --from-backup {spec}",
            out.display()
        );
    } else {
        println!("  Restore a machine: neo restore --from-backup {spec} --host root@<new machine>");
    }
    Ok(())
}
