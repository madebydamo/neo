//! The backup destination as `neo restore` reads it from settings.toml: the
//! same keys the backup service writes with (nix/services/backup, design
//! docs/design/backup-restore.md §2), used here in the other direction.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use toml_edit::DocumentMut;

use crate::utils::command::shell_quote;
use crate::utils::execute_command;

/// Prefix of the snapshots the backup service takes (backup.sh).
pub const SNAPSHOT_PREFIX: &str = "neo-backup-";

#[derive(Debug, Clone, PartialEq)]
pub enum Format {
    Files,
    Zfs,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Location {
    /// `user@host` and the ssh options from `extraOptions`.
    Ssh { dest: String, options: Vec<String> },
    /// A disk attached to the machine being restored.
    Disk,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub location: Location,
    pub format: Format,
    /// files: folder holding `<basename of sourceDir>/` (remotePath).
    pub remote_path: String,
    /// files: basename of sourceDir (`neo` for /var/neo).
    pub folder: String,
    /// zfs: dataset holding the backup snapshots.
    pub dataset: String,
    /// sshKey from the settings (the host key by default).
    pub ssh_key: String,
}

fn str_at<'a>(doc: &'a DocumentMut, path: &[&str]) -> Option<&'a str> {
    let mut item = doc.as_item();
    for p in path {
        item = item.get(p)?;
    }
    item.as_str()
}

/// neo.core.volumes.root (default /var/neo).
pub fn volumes_root(doc: &DocumentMut) -> String {
    str_at(doc, &["core", "volumes", "root"])
        .unwrap_or("/var/neo")
        .trim_end_matches('/')
        .to_string()
}

pub fn hostname(doc: &DocumentMut) -> String {
    str_at(doc, &["core", "hostname"])
        .unwrap_or("homeserver")
        .to_string()
}

impl Source {
    /// The backup configured in settings.toml, or None when backup is off.
    pub fn from_settings(doc: &DocumentMut) -> Result<Option<Source>> {
        let Some(b) = doc.get("services").and_then(|s| s.get("backup")) else {
            return Ok(None);
        };
        if !b.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Ok(None);
        }
        let get = |k: &str| b.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let format = match get("format").as_deref().unwrap_or("files") {
            "files" => Format::Files,
            "zfs" => Format::Zfs,
            other => bail!("services.backup.format = {other:?} is not files or zfs"),
        };
        let location = match get("target").as_deref().unwrap_or("ssh") {
            "ssh" => {
                let host = get("host")
                    .filter(|h| !h.is_empty())
                    .context("services.backup.host is not set")?;
                let user = get("user").unwrap_or_else(|| "root".into());
                let options = b
                    .get("extraOptions")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                Location::Ssh {
                    dest: format!("{user}@{host}"),
                    options,
                }
            }
            "disk" => Location::Disk,
            other => bail!("services.backup.target = {other:?} is not ssh or disk"),
        };
        let root = volumes_root(doc);
        let source_dir = get("sourceDir").unwrap_or_else(|| root.clone());
        let folder = Path::new(&source_dir)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "neo".into());
        if format == Format::Files && source_dir.trim_end_matches('/') != root {
            bail!(
                "the backup copies {source_dir}, not all of {root}: restore it by hand (docs/RESTORE.md)"
            );
        }
        Ok(Some(Source {
            location,
            format,
            remote_path: get("remotePath").unwrap_or_else(|| hostname(doc)),
            folder,
            dataset: get("dataset").unwrap_or_default(),
            ssh_key: get("sshKey").unwrap_or_else(|| "/etc/ssh/ssh_host_ed25519_key".into()),
        }))
    }

    pub fn describe(&self) -> String {
        let at = match &self.location {
            Location::Ssh { dest, .. } => dest.clone(),
            Location::Disk => "a disk on this machine".into(),
        };
        match self.format {
            Format::Files => format!("files in {}/{} on {at}", self.remote_path, self.folder),
            Format::Zfs => format!("ZFS snapshots of {} on {at}", self.dataset),
        }
    }
}

/// How this machine reaches the backup: the keys to try and a known_hosts file.
pub struct Access {
    pub identities: Vec<PathBuf>,
    pub known_hosts: PathBuf,
}

impl Access {
    /// `ssh` with the keys and the settings' extraOptions.
    pub fn ssh_args(&self, source: &Source) -> Vec<String> {
        let mut args = vec![
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            "IdentitiesOnly=yes".into(),
            "-o".into(),
            "StrictHostKeyChecking=accept-new".into(),
            "-o".into(),
            format!("UserKnownHostsFile={}", self.known_hosts.display()),
        ];
        for k in &self.identities {
            args.push("-i".into());
            args.push(k.display().to_string());
        }
        if let Location::Ssh { options, .. } = &source.location {
            args.extend(options.iter().cloned());
        }
        args
    }

    fn ssh_line(&self, source: &Source) -> String {
        std::iter::once("ssh".to_string())
            .chain(self.ssh_args(source).iter().map(|a| shell_quote(a)))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Run a command where the backup is (over SSH, or here) and capture stdout.
    pub fn output(&self, source: &Source, argv: &[&str]) -> Result<String> {
        let mut cmd = self.command(source, argv);
        let out = cmd
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("run {}", argv.join(" ")))?;
        if !out.status.success() {
            bail!(
                "{} failed: {}",
                argv.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    fn command(&self, source: &Source, argv: &[&str]) -> Command {
        match &source.location {
            Location::Ssh { dest, .. } => {
                let mut c = Command::new("ssh");
                c.args(self.ssh_args(source)).arg(dest).arg(
                    argv.iter()
                        .map(|a| shell_quote(a))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                c
            }
            Location::Disk => {
                let mut c = Command::new(argv[0]);
                c.args(&argv[1..]);
                c
            }
        }
    }
}

/// The newest backup snapshot in `dataset` at the destination.
pub fn newest_snapshot(access: &Access, source: &Source) -> Result<Option<String>> {
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
fn files_origin(source: &Source, data_from: Option<&Path>) -> Result<String> {
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
                        .with_context(|| format!("import the backup pool {pool}: is the disk attached?"))?;
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
            Ok(format!(
                "{}@{snap} ({})",
                source.dataset,
                used.trim().to_string()
            ))
        }
    }
}

/// Copy the backup into `<target><root>`, the freshly made data dataset.
/// `local_dataset` is the Neo data dataset (zfs format).
pub fn restore_into(
    access: &Access,
    source: &Source,
    data_from: Option<&Path>,
    target_root: &Path,
    config_rel: &str,
    local_dataset: &str,
    root: &str,
) -> Result<()> {
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
            rsync.arg(&origin).arg(format!("{}/", target_root.display()));
            execute_command(&mut rsync)
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
            execute_command(
                Command::new("mount")
                    .args(["-t", "zfs", "-o", "zfsutil", local_dataset])
                    .arg(target_root),
            )?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(s: &str) -> DocumentMut {
        s.parse().unwrap()
    }

    #[test]
    fn off_without_backup() {
        assert!(Source::from_settings(&doc("")).unwrap().is_none());
        assert!(
            Source::from_settings(&doc("[services.backup]\nhost = \"x\"\n"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn ssh_files_defaults() {
        let d = doc(
            "[core]\nhostname = \"myhome\"\n[services.backup]\nenabled = true\nhost = \"b.example\"\nuser = \"zh1\"\nextraOptions = [\"-p\", \"23\"]\n",
        );
        let s = Source::from_settings(&d).unwrap().unwrap();
        assert_eq!(s.format, Format::Files);
        assert_eq!(s.remote_path, "myhome");
        assert_eq!(s.folder, "neo");
        assert_eq!(s.ssh_key, "/etc/ssh/ssh_host_ed25519_key");
        assert_eq!(
            s.location,
            Location::Ssh {
                dest: "zh1@b.example".into(),
                options: vec!["-p".into(), "23".into()]
            }
        );
        assert_eq!(files_origin(&s, None).unwrap(), "zh1@b.example:myhome/neo/");
    }

    #[test]
    fn disk_zfs() {
        let d = doc(
            "[services.backup]\nenabled = true\ntarget = \"disk\"\nformat = \"zfs\"\ndataset = \"usb/neo\"\n",
        );
        let s = Source::from_settings(&d).unwrap().unwrap();
        assert_eq!(s.location, Location::Disk);
        assert_eq!(s.format, Format::Zfs);
        assert_eq!(s.dataset, "usb/neo");
    }

    #[test]
    fn partial_files_backup_is_refused() {
        let d = doc(
            "[services.backup]\nenabled = true\nhost = \"h\"\nsourceDir = \"/var/neo/DATA/Media\"\n",
        );
        assert!(Source::from_settings(&d).is_err());
    }
}
