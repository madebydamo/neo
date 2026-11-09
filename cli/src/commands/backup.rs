//! `neo backup`: the encryption key of the backup service and reading a
//! backup from elsewhere (docs/design/backup-restore.md §3).

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

use crate::commands::restore::backup::{self, Access, Source};
use crate::commands::restore::local::{commit_uncommitted, remote_url};
use crate::commands::restore::remote::default_key;
use crate::utils::command::root_command;
use crate::utils::execute_command;
use crate::utils::fsutil::TempDir;

const ROTATE_UNIT: &str = "backup-rotate-key.service";

/// Start backup-rotate-key.service: new data key, new repository, full backup,
/// old repository removed (backup.sh). It waits for a running backup.
pub fn rotate_key(doc: &DocumentMut, dry_run: bool, sudo_cmd: &str) -> Result<()> {
    let b = doc.get("services").and_then(|s| s.get("backup"));
    let enabled = b
        .and_then(|b| b.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let is_encrypted = |d: &toml_edit::Item| {
        d.get("enabled").and_then(|v| v.as_bool()) != Some(false)
            && d.get("format").and_then(|v| v.as_str()) == Some("encrypted")
    };
    // Every encrypted destination gets a new key; configs from before
    // `destinations` have the format at the top.
    let encrypted = match b
        .and_then(|b| b.get("destinations"))
        .and_then(|d| d.as_table_like())
    {
        Some(dests) => dests.iter().any(|(_, d)| is_encrypted(d)),
        None => b.is_some_and(is_encrypted),
    };
    if !enabled || !encrypted {
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

/// Copy the config repo out of a backup into `out`.
pub fn extract_config(
    spec: &str,
    out: &Path,
    key: Option<PathBuf>,
    backup_key: Option<PathBuf>,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: would copy the config repo from {spec} to {}",
            out.display()
        );
        return Ok(());
    }
    let key = key.or_else(default_key);
    let work = TempDir::new("neo-backup")?;
    let access = Access {
        identities: backup_key.into_iter().chain(key).collect(),
        known_hosts: work.join("known_hosts"),
        work: work.path().to_path_buf(),
    };
    let source = backup::detect(&access, Source::from_location(spec)?)?;
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
    if spec.starts_with('/') || spec.starts_with('.') {
        println!(
            "  Restore a machine (as root on it, with the disk attached): neo restore {} --from-backup {spec}",
            out.display()
        );
    } else {
        println!("  Restore a machine: neo restore --from-backup {spec} --host root@<new machine>");
    }
    Ok(())
}
