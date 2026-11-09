//! The backup destination as `neo restore` reads it from settings.toml: the
//! same keys the backup service writes with (nix/services/backup, design
//! docs/design/backup-restore.md §2), used here in the other direction.

use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use toml_edit::DocumentMut;

use crate::utils::command::{output_checked, output_with_stdin, shell_quote, stdout_checked};
use crate::utils::execute_command;
use crate::utils::fsutil::write_private;

/// Prefix of the snapshots the backup service takes (backup.sh).
pub const SNAPSHOT_PREFIX: &str = "neo-backup-";

/// Encrypted format: the data key next to the restic repository, encrypted
/// with age to the host key and the admin keys (backup.sh).
pub const HEADER: &str = "neo-backup.age";

/// The config repo below volumes.root unless `configPath` moves it.
pub const DEFAULT_CONFIG_REL: &str = "DATA/AppData/configuration";

#[derive(Debug, Clone, PartialEq)]
pub enum Format {
    Files,
    /// restic repository + neo-backup.age.
    Encrypted,
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
    /// The destination's name in the settings; empty for `--from-backup`.
    pub name: String,
    pub location: Location,
    pub format: Format,
    /// files: folder holding `<basename of sourceDir>/`; encrypted: folder
    /// holding neo-backup.age and the repository (remotePath).
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
    /// The enabled backup destinations in settings.toml, in the order a restore
    /// tries them: by `priority`, then name (the backup's own run order,
    /// nix/services/backup). Empty when backup is off. Configs from before
    /// `destinations` (migration 015) are one destination named `main`.
    pub fn from_settings(doc: &DocumentMut) -> Result<Vec<Source>> {
        let Some(b) = doc.get("services").and_then(|s| s.get("backup")) else {
            return Ok(Vec::new());
        };
        if !b.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Ok(Vec::new());
        }
        let root = volumes_root(doc);
        let source_dir = b
            .get("sourceDir")
            .and_then(|v| v.as_str())
            .unwrap_or(&root)
            .to_string();
        let mut found = Vec::new();
        match b.get("destinations").and_then(|d| d.as_table_like()) {
            Some(dests) => {
                for (name, d) in dests.iter() {
                    if d.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
                        continue;
                    }
                    let priority = d
                        .get("priority")
                        .and_then(|v| v.as_integer())
                        .unwrap_or(100);
                    found.push((priority, Self::destination(doc, name, d, &source_dir)?));
                }
            }
            None => found.push((0, Self::destination(doc, "main", b, &source_dir)?)),
        }
        found.sort_by(|(pa, a), (pb, b)| pa.cmp(pb).then_with(|| a.name.cmp(&b.name)));
        Ok(found.into_iter().map(|(_, s)| s).collect())
    }

    /// One destination table (`services.backup.destinations.<name>`).
    fn destination(
        doc: &DocumentMut,
        name: &str,
        b: &toml_edit::Item,
        source_dir: &str,
    ) -> Result<Source> {
        let at = format!("services.backup.destinations.{name}");
        let get = |k: &str| b.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let format = match get("format").as_deref().unwrap_or("files") {
            "files" => Format::Files,
            "encrypted" => Format::Encrypted,
            "zfs" => Format::Zfs,
            other => bail!("{at}.format = {other:?} is not files, encrypted or zfs"),
        };
        let location = match get("target").as_deref().unwrap_or("ssh") {
            "ssh" => {
                let host = get("host")
                    .filter(|h| !h.is_empty())
                    .with_context(|| format!("{at}.host is not set"))?;
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
            "store" => {
                let url = get("storeUrl")
                    .filter(|u| !u.is_empty())
                    .with_context(|| format!("{at}.storeUrl is not set"))?;
                let client = get("storeClient").unwrap_or_else(|| hostname(doc));
                store_location(&url, &client)?
            }
            other => bail!("{at}.target = {other:?} is not ssh, disk or store"),
        };
        let root = volumes_root(doc);
        let folder = Path::new(source_dir)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "neo".into());
        if format != Format::Zfs && source_dir.trim_end_matches('/') != root {
            bail!(
                "the backup copies {source_dir}, not all of {root}: restore it by hand (docs/RESTORE.md)"
            );
        }
        Ok(Source {
            name: name.to_string(),
            location,
            format,
            remote_path: get("remotePath").unwrap_or_else(|| hostname(doc)),
            folder,
            dataset: get("dataset").unwrap_or_default(),
            ssh_key: get("sshKey").unwrap_or_else(|| "/etc/ssh/ssh_host_ed25519_key".into()),
        })
    }

    /// A backup named on the command line (`--from-backup`): `user@host:folder`
    /// (relative to the home folder), `ssh://user@host[:port]/abs/folder`,
    /// `https://client@store[:port]/folder` (a backup store), or a local
    /// folder. The format is found with [`detect`].
    pub fn from_location(spec: &str) -> Result<Source> {
        let (location, remote_path) = if let Some(rest) = spec.strip_prefix("https://") {
            let (auth, path) = rest.split_once('/').unwrap_or((rest, ""));
            let (client, host) = auth.split_once('@').with_context(|| {
                format!("{spec}: name the server in the store: https://<name>@store/folder")
            })?;
            (
                store_location(&format!("https://{host}"), client)?,
                path.to_string(),
            )
        } else if spec.starts_with("http://") {
            bail!("{spec}: a backup store is only reachable over https://");
        } else if let Some(rest) = spec.strip_prefix("ssh://") {
            let (auth, path) = rest.split_once('/').unwrap_or((rest, ""));
            let (dest, options) = match auth.rsplit_once(':') {
                Some((d, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => {
                    (d.to_string(), vec!["-p".to_string(), p.to_string()])
                }
                _ => (auth.to_string(), Vec::new()),
            };
            (Location::Ssh { dest, options }, format!("/{path}"))
        } else if spec.starts_with('/') || spec.starts_with('.') {
            let p = fs::canonicalize(spec).with_context(|| format!("{spec} does not exist"))?;
            (Location::Disk, p.display().to_string())
        } else if let Some((dest, path)) = spec.split_once(':') {
            (
                Location::Ssh {
                    dest: dest.to_string(),
                    options: Vec::new(),
                },
                path.to_string(),
            )
        } else {
            bail!("{spec}: give the backup as user@host:folder, ssh://user@host:port/folder, https://name@store/folder or a local folder");
        };
        let remote_path = match remote_path.trim_end_matches('/') {
            "" if remote_path.starts_with('/') => "/".to_string(),
            p => p.to_string(),
        };
        Ok(Source {
            name: String::new(),
            location,
            format: Format::Files,
            remote_path,
            folder: "neo".into(),
            dataset: String::new(),
            ssh_key: String::new(),
        })
    }

    pub fn describe(&self) -> String {
        let at = match &self.location {
            Location::Ssh { dest, .. } => dest.clone(),
            Location::Disk => "a disk on this machine".into(),
        };
        let what = match self.format {
            Format::Files => format!("files in {}/{} on {at}", self.remote_path, self.folder),
            Format::Encrypted => format!(
                "encrypted backup in {} on {at}",
                folder_or_home(&self.remote_path)
            ),
            Format::Zfs => format!("ZFS snapshots of {} on {at}", self.dataset),
        };
        if self.name.is_empty() {
            what
        } else {
            format!("{} ({what})", self.name)
        }
    }
}

/// A backup store: SSH to the client's account, inside a WebSocket on `url`
/// (nix/services/backup-store).
fn store_location(url: &str, client: &str) -> Result<Location> {
    let authority = url
        .strip_prefix("https://")
        .with_context(|| format!("{url}: a backup store is only reachable over https://"))?
        .trim_end_matches('/');
    let host = authority.split(':').next().unwrap_or_default();
    if host.is_empty() || client.is_empty() || authority.contains('/') {
        bail!("{url} is not a backup store address like https://backup.example.com");
    }
    Ok(Location::Ssh {
        dest: format!("backup-{client}@{host}"),
        options: vec![
            "-o".into(),
            format!("ProxyCommand=websocat --binary -E - wss://{authority}/"),
            "-o".into(),
            "ServerAliveInterval=30".into(),
        ],
    })
}

fn folder_or_home(p: &str) -> &str {
    if p.is_empty() {
        "the home folder"
    } else {
        p
    }
}

/// `path` inside `dir`, as sftp and rsync read it ("" = the SSH user's home).
fn join_remote(dir: &str, path: &str) -> String {
    match dir.trim_end_matches('/') {
        "" if dir.starts_with('/') => format!("/{path}"),
        "" => path.to_string(),
        d => format!("{d}/{path}"),
    }
}

/// Which backup a `--from-backup` location holds: neo-backup.age (encrypted),
/// or a `neo` folder with DATA (plain files; the location may also be that
/// folder itself). ZFS backups need the config repo.
pub fn detect(access: &Access, mut source: Source) -> Result<Source> {
    let header = access
        .exists(&source, &join_remote(&source.remote_path, HEADER))
        .map_err(|e| {
            // A backup store's ZFS account has no files, so it refuses sftp.
            anyhow!("{e:#}. A ZFS backup needs the config repo: neo restore <repo>")
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

/// How this machine reaches the backup: the keys to try (for the SSH login and
/// for neo-backup.age), a known_hosts file and a private folder for the ssh
/// wrapper and the decrypted data key.
pub struct Access {
    pub identities: Vec<PathBuf>,
    pub known_hosts: PathBuf,
    pub work: PathBuf,
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

    /// An executable ssh with these keys and options, for `sftp -S` and
    /// restic's sftp.command (both pass ssh's own arguments).
    fn ssh_wrapper(&self, source: &Source) -> Result<PathBuf> {
        let path = self.work.join("ssh-wrapper");
        if path.to_string_lossy().contains(char::is_whitespace) {
            bail!("{} contains a space: restic cannot use it", path.display());
        }
        fs::create_dir_all(&self.work)?;
        fs::write(
            &path,
            format!("#!/bin/sh\nexec {} \"$@\"\n", self.ssh_line(source)),
        )?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        Ok(path)
    }

    /// Run an sftp batch at the backup (ssh only). Fails when a command fails.
    fn sftp(&self, source: &Source, batch: &str) -> Result<std::process::Output> {
        let Location::Ssh { dest, .. } = &source.location else {
            bail!("sftp needs an SSH backup");
        };
        output_with_stdin(
            Command::new("sftp")
                .arg("-q")
                .arg("-S")
                .arg(self.ssh_wrapper(source)?)
                .args(["-b", "-"])
                .arg(dest),
            batch.as_bytes(),
            "sftp",
        )
    }

    /// Whether `path` exists at the backup.
    pub fn exists(&self, source: &Source, path: &str) -> Result<bool> {
        match source.location {
            Location::Disk => Ok(Path::new(path).exists()),
            Location::Ssh { ref dest, .. } => {
                let out = self.sftp(source, &format!("ls -1 \"{path}\"\n"))?;
                if out.status.success() {
                    return Ok(true);
                }
                let err = String::from_utf8_lossy(&out.stderr);
                if err.contains("not found") || err.contains("No such file") {
                    return Ok(false);
                }
                bail!("cannot reach the backup at {dest}: {}", err.trim())
            }
        }
    }

    /// Copy the file `path` at the backup to `out`.
    fn fetch(&self, source: &Source, path: &str, out: &Path) -> Result<()> {
        match source.location {
            Location::Disk => {
                fs::copy(path, out).with_context(|| format!("read {path}"))?;
            }
            Location::Ssh { ref dest, .. } => {
                let o = self.sftp(source, &format!("get \"{path}\" \"{}\"\n", out.display()))?;
                if !o.status.success() {
                    bail!(
                        "cannot read {path} at {dest}: {}",
                        String::from_utf8_lossy(&o.stderr).trim()
                    );
                }
            }
        }
        Ok(())
    }

    /// Run a command where the backup is (over SSH, or here) and capture stdout.
    pub fn output(&self, source: &Source, argv: &[&str]) -> Result<String> {
        stdout_checked(&mut self.command(source, argv), &argv.join(" "))
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

/// The decrypted neo-backup.age: which repository, and its password.
#[derive(Debug, Clone, PartialEq)]
pub struct DataKey {
    pub repository: String,
    pub password: String,
}

impl DataKey {
    pub fn parse(text: &str) -> Result<DataKey> {
        let get = |k: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(&format!("{k}=")))
                .map(|v| v.trim().to_string())
        };
        if get("format").as_deref() != Some("restic") {
            bail!("{HEADER} is not a restic backup key (written by a newer neo?)");
        }
        let repository = get("repository")
            .filter(|r| r.starts_with("restic-") && !r.contains('/'))
            .with_context(|| format!("{HEADER} names no repository"))?;
        let password = get("password")
            .filter(|p| !p.is_empty())
            .with_context(|| format!("{HEADER} holds no password"))?;
        Ok(DataKey {
            repository,
            password,
        })
    }
}

/// An encrypted backup: where it is and how to open it.
pub struct Encrypted<'a> {
    access: &'a Access,
    source: Source,
    key: DataKey,
}

/// The encrypted backup's folder: `--data-from` (a folder on this machine)
/// or the settings.
fn encrypted_source(source: &Source, data_from: Option<&Path>) -> Source {
    match data_from {
        Some(p) => Source {
            location: Location::Disk,
            remote_path: p.display().to_string(),
            ..source.clone()
        },
        None => source.clone(),
    }
}

impl<'a> Encrypted<'a> {
    /// Fetch neo-backup.age and decrypt it with the first key that opens it.
    pub fn open(access: &'a Access, source: &Source, data_from: Option<&Path>) -> Result<Self> {
        let source = encrypted_source(source, data_from);
        fs::create_dir_all(&access.work)?;
        let header = access.work.join(HEADER);
        access.fetch(&source, &join_remote(&source.remote_path, HEADER), &header)?;
        let mut errors = Vec::new();
        for id in access.identities.iter().filter(|k| k.exists()) {
            let out = Command::new("age")
                .arg("-d")
                .arg("-i")
                .arg(id)
                .arg(&header)
                .stdin(Stdio::inherit())
                .output()
                .context("run age (is it installed?)")?;
            if out.status.success() {
                let key = DataKey::parse(&String::from_utf8_lossy(&out.stdout))?;
                return Ok(Encrypted {
                    access,
                    source,
                    key,
                });
            }
            errors.push(format!(
                "{}: {}",
                id.display(),
                String::from_utf8_lossy(&out.stderr)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
            ));
        }
        bail!(
            "none of the keys opens {HEADER} (it opens with the server's host key or an ssh-ed25519 key from core.ssh.authorizedKeys): {}",
            errors.join("; ")
        )
    }

    pub fn repository(&self) -> &str {
        &self.key.repository
    }

    /// `restic -r <repository>` with the password and, over SSH, the keys.
    fn restic(&self) -> Result<Command> {
        let pw = self.access.work.join("restic-password");
        write_private(&pw, self.key.password.as_bytes())?;
        let dir = join_remote(&self.source.remote_path, &self.key.repository);
        let mut c = Command::new("restic");
        c.env("RESTIC_PASSWORD_FILE", &pw).arg("--no-cache");
        match &self.source.location {
            Location::Disk => {
                c.arg("-r").arg(&dir);
            }
            Location::Ssh { dest, .. } => {
                let wrapper = self.access.ssh_wrapper(&self.source)?;
                c.arg("-r")
                    .arg(format!("sftp:{dest}:{dir}"))
                    .arg("-o")
                    .arg(format!("sftp.command={} {dest} -s sftp", wrapper.display()));
            }
        }
        Ok(c)
    }

    /// The newest snapshot: (time, size line).
    pub fn latest(&self) -> Result<String> {
        let out = output_checked(
            self.restic()?
                .args(["snapshots", "--tag", "neo", "--latest", "1", "--json"]),
            "restic snapshots",
        )?;
        let v: serde_json::Value =
            serde_json::from_slice(&out.stdout).context("parse restic snapshots")?;
        let snap = v
            .as_array()
            .and_then(|a| {
                a.iter()
                    .max_by_key(|s| s["time"].as_str().unwrap_or("").to_string())
            })
            .with_context(|| format!("{} holds no backup yet", self.key.repository))?;
        let time = snap["time"].as_str().unwrap_or("?");
        let time = time.split('.').next().unwrap_or(time).replace('T', " ");
        let size = snap["summary"]["total_bytes_processed"]
            .as_u64()
            .map(|b| format!(", {:.1} GiB", b as f64 / (1u64 << 30) as f64))
            .unwrap_or_default();
        Ok(format!("snapshot of {time}{size}"))
    }

    /// Restore the newest snapshot into `target`, leaving out `exclude`
    /// (relative paths), or only `include`.
    fn restore(&self, target: &Path, exclude: &[&str], include: &[&str]) -> Result<()> {
        let mut c = self.restic()?;
        c.args(["restore", "latest", "--tag", "neo", "--target"])
            .arg(target);
        for e in exclude {
            c.arg(format!("--exclude=/{e}"));
        }
        for i in include {
            c.arg(format!("--include=/{i}"));
        }
        execute_command(&mut c)
    }

    /// Folders of the newest snapshot that look like a config repo
    /// (settings.toml next to .git), shortest first.
    fn config_repos(&self) -> Result<Vec<String>> {
        let text = stdout_checked(
            self.restic()?.args(["ls", "latest", "--tag", "neo"]),
            "restic ls",
        )?;
        let paths: std::collections::HashSet<&str> = text.lines().collect();
        let mut found: Vec<String> = paths
            .iter()
            .filter_map(|p| p.strip_suffix("/settings.toml"))
            .filter(|d| paths.contains(format!("{d}/.git").as_str()))
            .map(|d| d.trim_start_matches('/').to_string())
            .collect();
        found.sort_by_key(|d| (d.len(), d.clone()));
        Ok(found)
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
            if !stage.join(&rel).join("settings.toml").exists() {
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
            if !out.join("settings.toml").exists() {
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
        assert!(Source::from_settings(&doc("")).unwrap().is_empty());
        assert!(
            Source::from_settings(&doc("[services.backup]\nhost = \"x\"\n"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn ssh_files_defaults() {
        let d = doc(
            "[core]\nhostname = \"myhome\"\n[services.backup]\nenabled = true\nhost = \"b.example\"\nuser = \"zh1\"\nextraOptions = [\"-p\", \"23\"]\n",
        );
        let s = Source::from_settings(&d).unwrap().remove(0);
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
        let s = Source::from_settings(&d).unwrap().remove(0);
        assert_eq!(s.location, Location::Disk);
        assert_eq!(s.format, Format::Zfs);
        assert_eq!(s.dataset, "usb/neo");
    }

    #[test]
    fn encrypted_from_settings() {
        let d = doc("[services.backup]\nenabled = true\nformat = \"encrypted\"\nhost = \"b\"\nremotePath = \"bk/home\"\n");
        let s = Source::from_settings(&d).unwrap().remove(0);
        assert_eq!(s.format, Format::Encrypted);
        assert_eq!(
            join_remote(&s.remote_path, HEADER),
            "bk/home/neo-backup.age"
        );
        assert!(s
            .describe()
            .starts_with("main (encrypted backup in bk/home"));
    }

    #[test]
    fn locations() {
        let s = Source::from_location("zh1@b.example:backups/home/").unwrap();
        assert_eq!(
            s.location,
            Location::Ssh {
                dest: "zh1@b.example".into(),
                options: vec![]
            }
        );
        assert_eq!(s.remote_path, "backups/home");
        let s = Source::from_location("ssh://root@10.0.0.2:2222/tank/bk").unwrap();
        assert_eq!(
            s.location,
            Location::Ssh {
                dest: "root@10.0.0.2".into(),
                options: vec!["-p".into(), "2222".into()]
            }
        );
        assert_eq!(s.remote_path, "/tank/bk");
        let s = Source::from_location("ssh://u@h").unwrap();
        assert_eq!(s.remote_path, "/");
        assert_eq!(join_remote(&s.remote_path, HEADER), "/neo-backup.age");
        let s = Source::from_location("u@h:").unwrap();
        assert_eq!(join_remote(&s.remote_path, HEADER), "neo-backup.age");
        assert_eq!(Source::from_location("/").unwrap().location, Location::Disk);
        assert!(Source::from_location("justaname").is_err());
    }

    #[test]
    fn store() {
        let d = doc("[core]\nhostname = \"myhome\"\n[services.backup]\nenabled = true\ntarget = \"store\"\nformat = \"encrypted\"\nstoreUrl = \"https://backup.example.com/\"\n");
        let s = Source::from_settings(&d).unwrap().remove(0);
        let Location::Ssh { dest, options } = &s.location else {
            panic!();
        };
        assert_eq!(dest, "backup-myhome@backup.example.com");
        assert_eq!(
            options[1],
            "ProxyCommand=websocat --binary -E - wss://backup.example.com/"
        );

        let s = Source::from_location("https://myhome@backup.example.com:8443/dev/").unwrap();
        let Location::Ssh { dest, options } = &s.location else {
            panic!();
        };
        assert_eq!(dest, "backup-myhome@backup.example.com");
        assert!(options[1].ends_with("wss://backup.example.com:8443/"));
        assert_eq!(s.remote_path, "dev");
        assert_eq!(
            join_remote(
                &Source::from_location("https://m@b.example")
                    .unwrap()
                    .remote_path,
                HEADER
            ),
            HEADER
        );

        assert!(Source::from_location("https://b.example/dev").is_err());
        assert!(Source::from_location("http://m@b.example/dev").is_err());
        assert!(store_location("http://b.example", "m").is_err());
    }

    #[test]
    fn destinations_in_restore_order() {
        let d = doc(concat!(
            "[core]\nhostname = \"myhome\"\n",
            "[services.backup]\nenabled = true\n",
            "[services.backup.destinations.usb]\ntarget = \"disk\"\nremotePath = \"/mnt/usb\"\n",
            "[services.backup.destinations.friend-a]\nhost = \"b.example\"\nremotePath = \"disk1/neo\"\nformat = \"encrypted\"\npriority = 10\n",
            "[services.backup.destinations.friend-b]\nhost = \"b.example\"\nremotePath = \"disk2/neo\"\nformat = \"encrypted\"\npriority = 10\n",
            "[services.backup.destinations.old]\nenabled = false\nhost = \"gone.example\"\n",
        ));
        let all = Source::from_settings(&d).unwrap();
        let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["friend-a", "friend-b", "usb"]);
        assert_eq!(all[0].remote_path, "disk1/neo");
        assert_eq!(all[1].remote_path, "disk2/neo");
        assert_eq!(all[2].location, Location::Disk);
        assert!(all[0]
            .describe()
            .starts_with("friend-a (encrypted backup in disk1/neo"));
        // The flat keys of configs from before are one destination.
        let legacy = doc("[services.backup]\nenabled = true\nhost = \"h\"\n");
        assert_eq!(Source::from_settings(&legacy).unwrap()[0].name, "main");
    }

    #[test]
    fn data_key() {
        let k = DataKey::parse(
            "version=1\nformat=restic\nrepository=restic-20261103-120000\npassword=abc=\n",
        )
        .unwrap();
        assert_eq!(k.repository, "restic-20261103-120000");
        assert_eq!(k.password, "abc=");
        assert!(DataKey::parse("format=restic\nrepository=../x\npassword=a\n").is_err());
        assert!(DataKey::parse("format=zfs\nrepository=restic-1\npassword=a\n").is_err());
    }

    #[test]
    fn partial_files_backup_is_refused() {
        let d = doc(
            "[services.backup]\nenabled = true\nhost = \"h\"\nsourceDir = \"/var/neo/DATA/Media\"\n",
        );
        assert!(Source::from_settings(&d).is_err());
    }
}
