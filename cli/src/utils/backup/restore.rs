//! Reading a backup: which format a location holds, whether it can be read,
//! the config repo out of it and the data into a new machine.

use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::access::Access;
use super::encrypted::{Encrypted, HEADER};
use super::source::{join_remote, Format, Location, Source};
use crate::utils::command::execute_command;
use crate::utils::profile::DEFAULT_CONFIG_REL;
use crate::utils::settings::FILE as SETTINGS_FILE;

/// Prefix of the snapshots the backup service takes (backup.sh).
pub const SNAPSHOT_PREFIX: &str = "neo-backup-";

/// Which backup a `--from-backup` location holds: neo-backup.age (encrypted),
/// or a `neo` folder with DATA (plain files; the location may also be that
/// folder itself). ZFS backups need the config repo.
pub fn detect(access: &Access, mut source: Source) -> Result<Source> {
    let header = access
        .exists(&source, &join_remote(&source.remote_path, HEADER))
        .map_err(|e| {
            let e = format!("{e:#}");
            if e.contains("Permission denied") || e.contains("Host key") {
                return anyhow!(e);
            }
            // A backup store's ZFS account has no files, so it refuses sftp.
            anyhow!("{e}. A ZFS backup needs the config repo: neo restore <repo>")
        })?;
    if header {
        source.format = Format::Encrypted;
        return Ok(source);
    }
    if access.exists(&source, &join_remote(&source.remote_path, "neo/DATA"))? {
        return Ok(source);
    }
    if access.exists(&source, &join_remote(&source.remote_path, "DATA"))? {
        let p = Path::new(&source.remote_path);
        source.folder = p
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .context("give the folder holding the backup, not /")?;
        source.remote_path = p
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        return Ok(source);
    }
    bail!(
        "no backup at {}: neither {HEADER} (encrypted) nor neo/DATA (plain files) is there. A ZFS backup needs the config repo: neo restore <repo>",
        source.remote_path
    )
}

/// The newest backup snapshot in `dataset` at the destination.
fn newest_snapshot(access: &Access, source: &Source) -> Result<Option<String>> {
    let out = access.output(
        source,
        &[
            "zfs",
            "list",
            "-H",
            "-t",
            "snapshot",
            "-o",
            "name",
            "-S",
            "createtxg",
            "-d",
            "1",
            &source.dataset,
        ],
    )?;
    Ok(out
        .lines()
        .filter_map(|l| l.split_once('@').map(|(_, s)| s.to_string()))
        .find(|s| s.starts_with(SNAPSHOT_PREFIX)))
}

/// Where a `files` backup is read from: `--data-from` or the settings.
pub(super) fn files_origin(source: &Source, data_from: Option<&Path>) -> Result<String> {
    if let Some(p) = data_from {
        // The folder holding DATA/ (the `neo` folder) or its parent.
        let p = if p.join("DATA").is_dir() {
            p.to_path_buf()
        } else {
            p.join(&source.folder)
        };
        if !p.join("DATA").is_dir() {
            bail!("{} holds no DATA folder", p.display());
        }
        return Ok(format!("{}/", p.display()));
    }
    match &source.location {
        Location::Ssh { dest, .. } => {
            let remote = if source.remote_path.is_empty() {
                source.folder.clone()
            } else {
                format!("{}/{}", source.remote_path, source.folder)
            };
            Ok(format!("{dest}:{remote}/"))
        }
        Location::Disk => {
            let p = Path::new(&source.remote_path).join(&source.folder);
            if !p.join("DATA").is_dir() {
                bail!(
                    "the backup disk is not mounted at {} (mount it and rerun, or pass --data-from <folder>)",
                    source.remote_path
                );
            }
            Ok(format!("{}/", p.display()))
        }
    }
}

/// Copy the config repo out of the backup into `out` (must not exist).
/// Returns the repo's folder relative to the backed-up volume.
pub fn extract_config(access: &Access, source: &Source, out: &Path) -> Result<String> {
    if out.exists() {
        bail!("{} exists already", out.display());
    }
    let parent = out.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    match source.format {
        Format::Encrypted => {
            let enc = Encrypted::open(access, source, None)?;
            let stage = access.work.join("config-extract");
            let _ = fs::remove_dir_all(&stage);
            let mut rel = DEFAULT_CONFIG_REL.to_string();
            enc.restore(&stage, &[], &[&rel])?;
            if !stage.join(&rel).join(SETTINGS_FILE).exists() {
                rel =
                    enc.config_repos()?.into_iter().next().context(
                        "the backup holds no config repo (no settings.toml next to .git)",
                    )?;
                enc.restore(&stage, &[], &[&rel])?;
            }
            move_dir(&stage.join(&rel), out)?;
            let _ = fs::remove_dir_all(&stage);
            Ok(rel)
        }
        Format::Files => {
            let origin = files_origin(source, None)?;
            let mut rsync = Command::new("rsync");
            rsync.args(["-a", "--info=stats1"]);
            if let Location::Ssh { .. } = source.location {
                rsync.arg("-e").arg(access.ssh_line(source));
            }
            rsync
                .arg(format!("{origin}{DEFAULT_CONFIG_REL}/"))
                .arg(format!("{}/", out.display()));
            execute_command(&mut rsync).with_context(|| {
                format!("no config repo at {origin}{DEFAULT_CONFIG_REL} in the backup")
            })?;
            if !out.join(SETTINGS_FILE).exists() {
                bail!("{origin}{DEFAULT_CONFIG_REL} holds no settings.toml");
            }
            Ok(DEFAULT_CONFIG_REL.to_string())
        }
        Format::Zfs => {
            bail!("a ZFS backup cannot be read without its config repo: neo restore <repo>")
        }
    }
}

fn move_dir(from: &Path, to: &Path) -> Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    execute_command(Command::new("cp").arg("-a").arg(from).arg(to))?;
    fs::remove_dir_all(from)?;
    Ok(())
}

/// Preflight: the backup can be read. Returns a line for the plan.
pub fn check(access: &Access, source: &Source, data_from: Option<&Path>) -> Result<String> {
    match source.format {
        Format::Files => {
            let origin = files_origin(source, data_from)?;
            if data_from.is_none() {
                if let Location::Ssh { .. } = source.location {
                    let dir = origin.split_once(':').map(|(_, d)| d).unwrap_or(&origin);
                    access
                        .output(source, &["test", "-d", &format!("{dir}DATA")])
                        .with_context(|| format!("no backup at {origin}DATA"))?;
                }
            }
            Ok(format!("files from {origin}"))
        }
        Format::Encrypted => {
            let enc = Encrypted::open(access, source, data_from)?;
            Ok(format!("{}, {}", enc.repository(), enc.latest()?))
        }
        Format::Zfs => {
            if source.location == Location::Disk {
                let pool = source.dataset.split('/').next().unwrap_or_default();
                let listed = Command::new("zpool")
                    .args(["list", "-H", pool])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .is_ok_and(|s| s.success());
                if !listed {
                    execute_command(Command::new("zpool").args(["import", "-N", pool]))
                        .with_context(|| {
                            format!("import the backup pool {pool}: is the disk attached?")
                        })?;
                }
            }
            let snap = newest_snapshot(access, source)?
                .with_context(|| format!("no {SNAPSHOT_PREFIX}* snapshot in {}", source.dataset))?;
            let used = access
                .output(
                    source,
                    &[
                        "zfs",
                        "get",
                        "-H",
                        "-o",
                        "value",
                        "referenced",
                        &format!("{}@{snap}", source.dataset),
                    ],
                )
                .unwrap_or_default();
            Ok(format!("{}@{snap} ({})", source.dataset, used.trim()))
        }
    }
}

/// Mount a ZFS dataset below the target, whatever its own mountpoint says.
pub(crate) fn mount_zfs(dataset: &str, path: &Path) -> Result<()> {
    execute_command(
        Command::new("mount")
            .args(["-t", "zfs", "-o", "zfsutil", dataset])
            .arg(path),
    )
}

/// Where [`restore_into`] puts the data.
pub struct RestoreInto<'a> {
    /// `--data-from`: a folder on this machine read instead of the destination.
    pub data_from: Option<&'a Path>,
    /// `<target><root>`, the freshly made data dataset.
    pub target_root: &'a Path,
    /// The config repo relative to `root`; left out (the one from git is installed).
    pub config_rel: &'a str,
    /// The Neo data dataset (zfs format).
    pub local_dataset: &'a str,
    /// `core.volumes.root` of the restored machine.
    pub root: &'a str,
}

/// Copy the backup into `<target><root>`, the freshly made data dataset.
pub fn restore_into(access: &Access, source: &Source, into: &RestoreInto) -> Result<()> {
    let RestoreInto {
        data_from,
        target_root,
        config_rel,
        local_dataset,
        root,
    } = *into;
    match source.format {
        Format::Files => {
            let origin = files_origin(source, data_from)?;
            let mut rsync = Command::new("rsync");
            rsync
                .args(["-aH", "--numeric-ids", "--info=progress2,stats2"])
                .arg(format!("--exclude=/{config_rel}/"));
            if let Location::Ssh { .. } = source.location {
                if data_from.is_none() {
                    rsync.arg("-e").arg(access.ssh_line(source));
                }
            }
            rsync
                .arg(&origin)
                .arg(format!("{}/", target_root.display()));
            execute_command(&mut rsync)
        }
        Format::Encrypted => {
            let enc = Encrypted::open(access, source, data_from)?;
            enc.restore(target_root, &[config_rel], &[])
        }
        Format::Zfs => {
            let snap = newest_snapshot(access, source)?
                .with_context(|| format!("no backup snapshot in {}", source.dataset))?;
            let full = format!("{}@{snap}", source.dataset);
            println!("→ zfs send {full} | zfs recv -F {local_dataset}");
            // Unmounted while it is replaced; mounted again below.
            let _ = Command::new("umount").arg(target_root).status();
            let mut send = access.command(source, &["zfs", "send", &full]);
            let mut sender = send
                .stdout(Stdio::piped())
                .spawn()
                .context("start zfs send")?;
            let stdout = sender.stdout.take().context("zfs send stdout")?;
            let recv = Command::new("zfs")
                .args(["recv", "-u", "-F"])
                .args(["-o", &format!("mountpoint={root}")])
                .args(["-o", "com.sun:auto-snapshot=true"])
                .args(["-o", "canmount=on"])
                .arg(local_dataset)
                .stdin(stdout)
                .status()
                .context("run zfs recv")?;
            let sent = sender.wait()?;
            if !sent.success() || !recv.success() {
                bail!("zfs send/recv of {full} failed");
            }
            mount_zfs(local_dataset, target_root)?;
            // The backup's copy of the config repo: the one from git is installed.
            let cfg = target_root.join(config_rel);
            if cfg.exists() {
                let aside = PathBuf::from(format!("{}.from-backup", cfg.display()));
                let _ = std::fs::remove_dir_all(&aside);
                std::fs::rename(&cfg, &aside)
                    .with_context(|| format!("move {} aside", cfg.display()))?;
                println!("  the backup's config repo is kept at {}", aside.display());
            }
            Ok(())
        }
    }
}
