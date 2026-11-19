//! `neo backup-store rescue <client> --host root@IP --key-file …`: restore a
//! client onto a new machine from its backup store
//! (docs/design/backup-store-rescue.md §4). Runs as root, in a transient unit
//! the web portal starts (`systemd-run -p RuntimeDirectory=…`).
//!
//! 1. The admin key moves into this job's private folder (the original is
//!    shredded); a passphrase is removed from the copy there.
//! 2. The config repo comes from the local copy of the backup.
//! 3. A short-lived key may read the client's backup over the tunnel
//!    (`neo-store-gate --read-only`), for the new machine.
//! 4. `neo restore <repo> --host … --from-backup https://<client>@<store>/…`
//!    (remote mode, the code a laptop runs).
//! 5. The short-lived key goes; the keys are shredded.

use anyhow::{bail, Context, Result};
use clap::Parser;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{now, Report, StoreClient, StoreConfig, RECENT_CONNECTION_SECS, RESCUE_KEYS_DIR};
use crate::commands::restore::{self, RestoreArgs};
use crate::utils::backup::config_repo::{commit_uncommitted, remote_url};
use crate::utils::backup::restore::SNAPSHOT_PREFIX;
use crate::utils::backup::{self, Access, Source};
use crate::utils::command::{execute_command, stdout_checked};
use crate::utils::fsutil::{write_private, TempDir};
use crate::utils::locks::{LockManager, LockMode, LockSpec, OpInfo, Scope};
use crate::utils::ops::{OpStatus, OperationKind, OperationLog};
use crate::utils::profile::DEFAULT_CONFIG_REL;

#[derive(clap::Args, Debug, Clone)]
pub struct RescueArgs {
    /// The client (its name in the store's settings).
    pub client: String,
    /// The new machine: root@IP[:PORT].
    #[arg(long)]
    pub host: String,
    /// Disk to install onto (sets disko.mainDisk), e.g. /dev/disk/by-id/….
    #[arg(long)]
    pub disk: Option<String>,
    /// Admin ssh-ed25519 private key. Moved into the job's private folder and
    /// shredded where it was.
    #[arg(long)]
    pub key_file: PathBuf,
    /// File with the key's passphrase (shredded after use).
    #[arg(long)]
    pub passphrase_file: Option<PathBuf>,
    /// Backup folder in the client's home, when it has more than one.
    #[arg(long)]
    pub folder: Option<String>,
    /// Only check and print the plan (boots the new machine into the installer).
    #[arg(long)]
    pub check: bool,
    /// Operation id suffix (rescue_<suffix>), for the web portal's progress.
    #[arg(long)]
    pub op: Option<String>,
}

/// Lock of one client's rescue: two at once would fight over its short-lived key.
pub fn lock_spec(client: &str) -> LockSpec {
    LockSpec::new().with(
        Scope::service(&format!("backup-store-rescue-{client}")),
        LockMode::Exclusive,
    )
}

pub fn run(args: &RescueArgs) -> Result<()> {
    if !super::client_name_ok(&args.client) {
        bail!("{} is not a client name", args.client);
    }
    let suffix = args
        .op
        .clone()
        .unwrap_or_else(crate::utils::command::get_timestamp);
    let op = OperationLog::new(OperationKind::Rescue, &suffix);
    let _tee = op.capture_stdio();
    let label = format!("Rescue of {}", args.client);
    let lock = LockManager::system()
        .try_acquire(
            &lock_spec(&args.client),
            &OpInfo::new("rescue", &label).with_op_id(op.id()),
        )
        .map_err(|e| anyhow::anyhow!("{e}"));
    let lock = match lock {
        Ok(l) => l,
        Err(e) => {
            op.write_state(OpStatus::Failed, "start", Some(&format!("{e:#}")), None);
            shred(&args.key_file);
            if let Some(p) = &args.passphrase_file {
                shred(p);
            }
            return Err(e);
        }
    };
    let result = rescue(args, &op);
    drop(lock);
    match &result {
        // `step` records a failure with its phase.
        Ok(()) => op.write_state(OpStatus::Success, "complete", None, None),
        Err(e) => println!("✗ {e:#}"),
    }
    result
}

fn rescue(args: &RescueArgs, op: &OperationLog) -> Result<()> {
    // Private: systemd's RuntimeDirectory (tmpfs, 0700), else a temp folder.
    let tmp;
    let work: PathBuf = match std::env::var("RUNTIME_DIRECTORY") {
        Ok(d) if !d.is_empty() => PathBuf::from(d.split(':').next().unwrap_or(&d)),
        _ => {
            tmp = TempDir::new("neo-rescue")?;
            tmp.path().to_path_buf()
        }
    };
    fs::set_permissions(&work, fs::Permissions::from_mode(0o700))?;
    let keys = KeyFiles {
        admin: work.join("admin_key"),
        backup: work.join("backup_key"),
    };

    let (store, client, source_spec, folder) = op.step("prepare", || {
        take_key(args, &work, &keys.admin)?;
        let store = StoreConfig::load()?;
        let client = store
            .clients
            .get(&args.client)
            .with_context(|| format!("{} is not a client of this store", args.client))?
            .clone();
        if !store.internet {
            println!("! This store is not open to the internet (Access: no \"web\"): the new machine must reach {} from your network or tailnet.", store.url);
        }
        warn_if_running(&args.client);
        let folder = match client.format.as_str() {
            "zfs" => None,
            _ => Some(pick_folder(&store, &args.client, args.folder.as_deref())?),
        };
        let spec = folder.as_ref().map(|f| store.backup_url(&args.client, f));
        Ok((store, client, spec, folder))
    })?;

    let repo = work.join("config");
    op.step("config", || {
        println!(
            "Taking the config repo of {} out of its backup here …",
            args.client
        );
        match folder.as_deref() {
            Some(f) => local_config(&store, &args.client, f, &keys.admin, &work, &repo),
            None => zfs_config(&store, &args.client, &work, &repo),
        }?;
        commit_uncommitted(&repo)?;
        match remote_url(&repo).as_str() {
            "" => println!("  The config repo has no remote."),
            url => println!("  Config repo from the backup (its remote: {url})"),
        }
        Ok(())
    })?;

    op.step("restore", || {
        let _short_lived = ShortLivedKey::add(&store, &args.client, &client, &keys.backup)?;
        let restore_args = restore_args(args, &repo, source_spec.as_deref(), &keys)?;
        if source_spec.is_none() {
            println!("  ZFS backup: the new machine takes its data from the backup places in its settings, in their order.");
        }
        restore::restore(&restore_args, false)
    })?;
    if args.check {
        println!("✓ Check done: nothing was erased. Start the restore when the plan is right.");
    } else {
        println!("✓ {} is restored on {}.", args.client, args.host);
        println!(
            "  Next: point its DNS names (and the streamproxy edge, if any) to the new address."
        );
    }
    Ok(())
}

/// The admin key (and its passphrase) in the job's folder; the originals are
/// shredded at once, whatever happens next.
fn take_key(args: &RescueArgs, work: &Path, admin: &Path) -> Result<()> {
    let copied = fs::read(&args.key_file)
        .with_context(|| format!("read {}", args.key_file.display()))
        .and_then(|k| write_private(admin, &k));
    shred(&args.key_file);
    let pass = args.passphrase_file.as_ref().map(|p| {
        let v = fs::read(p);
        shred(p);
        v
    });
    copied?;
    if let Some(pass) = pass {
        let pass = pass.context("read the passphrase")?;
        let pass_file = work.join("passphrase");
        write_private(&pass_file, &pass)?;
        let askpass = work.join("askpass");
        write_private(
            &askpass,
            format!(
                "#!/bin/sh\nexec cat {}\n",
                sh_quote(&pass_file.display().to_string())
            )
            .as_bytes(),
        )?;
        fs::set_permissions(&askpass, fs::Permissions::from_mode(0o700))?;
        let ok = Command::new("ssh-keygen")
            .args(["-p", "-q", "-N", "", "-f"])
            .arg(admin)
            .env("SSH_ASKPASS", &askpass)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("DISPLAY", ":0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        shred(&pass_file);
        let _ = fs::remove_file(&askpass);
        if !ok {
            bail!("the passphrase does not open the key");
        }
    }
    let public = Command::new("ssh-keygen")
        .args(["-y", "-f"])
        .arg(admin)
        .env("SSH_ASKPASS_REQUIRE", "never")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .context("ssh-keygen")?;
    if !public.status.success() {
        bail!("this is not a private SSH key, or it needs its passphrase");
    }
    if !String::from_utf8_lossy(&public.stdout).starts_with("ssh-ed25519 ") {
        bail!("give an ssh-ed25519 key: only those open the secrets and the backup");
    }
    Ok(())
}

/// Overwrite, then delete. tmpfs and copy-on-write file systems may keep old
/// blocks; the key is only ever in /run.
fn shred(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let _ = fs::write(path, vec![0u8; meta.len() as usize]);
    }
    let _ = fs::remove_file(path);
}

struct KeyFiles {
    admin: PathBuf,
    backup: PathBuf,
}

impl Drop for KeyFiles {
    fn drop(&mut self) {
        shred(&self.admin);
        shred(&self.backup);
        let _ = fs::remove_file(self.backup.with_extension("pub"));
    }
}

fn warn_if_running(client: &str) {
    let Ok(report) = Report::load() else {
        return;
    };
    if let Some(t) = report.client(client).and_then(|c| c.last_connection) {
        let ago = now() - t;
        if ago < RECENT_CONNECTION_SECS {
            let when = match ago / 60 {
                0 => "just now".to_string(),
                1 => "1 minute ago".to_string(),
                m => format!("{m} minutes ago"),
            };
            println!("! {client} connected to this store {when}: if the old server still runs, turn it off, or both back up into the same place.");
        }
    }
}

/// The backup folder to restore from: given, or the only one there is.
fn pick_folder(store: &StoreConfig, client: &str, wanted: Option<&str>) -> Result<String> {
    let report = Report::load().ok();
    let folders: Vec<String> = report
        .as_ref()
        .and_then(|r| r.client(client))
        .map(|c| c.folders.clone())
        .unwrap_or_default();
    if let Some(w) = wanted {
        let w = w.trim_matches('/');
        if w.split('/').any(|p| p == "..") || w.starts_with('-') {
            bail!("{w} is not a folder of {client}");
        }
        if !store.home(client).join(w).is_dir() {
            bail!("{client} has no folder {w}");
        }
        return Ok(if w.is_empty() {
            ".".into()
        } else {
            w.to_string()
        });
    }
    match folders.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!("{client} holds no backup yet"),
        many => bail!(
            "{client} has more than one backup folder ({}): pick one",
            many.join(", ")
        ),
    }
}

/// files / encrypted: the config repo from the backup folder on this machine.
fn local_config(
    store: &StoreConfig,
    client: &str,
    folder: &str,
    admin: &Path,
    work: &Path,
    out: &Path,
) -> Result<()> {
    let dir = store.home(client).join(folder);
    let access = Access {
        identities: vec![admin.to_path_buf()],
        known_hosts: work.join("known_hosts"),
        work: work.join("backup"),
    };
    fs::create_dir_all(&access.work)?;
    let source = backup::detect(&access, Source::from_location(&dir.display().to_string())?)?;
    backup::extract_config(&access, &source, out)?;
    let _ = fs::remove_dir_all(&access.work);
    Ok(())
}

/// zfs: the config repo from the newest backup snapshot, mounted read-only.
fn zfs_config(store: &StoreConfig, client: &str, work: &Path, out: &Path) -> Result<()> {
    let parent = store
        .client_dataset(client)
        .context("ZFS backups need the store's dataset storage")?;
    let list = stdout_checked(
        Command::new("zfs").args([
            "list",
            "-H",
            "-p",
            "-r",
            "-t",
            "snapshot",
            "-o",
            "name,creation",
            "-s",
            "creation",
            &parent,
        ]),
        "zfs list",
    )?;
    let snap = list
        .lines()
        .filter_map(|l| l.split('\t').next())
        .rfind(|n| {
            n.split_once('@')
                .is_some_and(|(_, s)| s.starts_with(SNAPSHOT_PREFIX))
        })
        .with_context(|| format!("{client} holds no backup snapshot yet"))?
        .to_string();
    println!("  from {snap}");
    let mnt = work.join("snapshot");
    fs::create_dir_all(&mnt)?;
    execute_command(
        Command::new("mount")
            .args(["-t", "zfs", "-o", "ro", &snap])
            .arg(&mnt),
    )?;
    let copied = (|| {
        let from = mnt.join(DEFAULT_CONFIG_REL);
        if !from.join(crate::utils::settings::FILE).exists() {
            bail!("{snap} holds no config repo at {DEFAULT_CONFIG_REL}");
        }
        execute_command(Command::new("cp").arg("-a").arg(&from).arg(out))
    })();
    let _ = Command::new("umount").arg(&mnt).status();
    copied
}

/// The short-lived key of the new machine: may only read this client's backup.
/// Removed when dropped.
struct ShortLivedKey {
    file: PathBuf,
}

impl ShortLivedKey {
    fn add(store: &StoreConfig, client: &str, c: &StoreClient, key: &Path) -> Result<Self> {
        let _ = fs::remove_file(key);
        execute_command(
            Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-C"])
                .arg(format!("neo-rescue-{client}"))
                .arg("-f")
                .arg(key),
        )?;
        let public = fs::read_to_string(key.with_extension("pub"))?;
        let dir = Path::new(RESCUE_KEYS_DIR);
        fs::create_dir_all(dir)?;
        let file = dir.join(format!("backup-{client}"));
        let line = format!(
            "restrict,command=\"{} --read-only {} {}\" {}\n",
            store.gate,
            c.format,
            c.gate_prefix,
            public.trim()
        );
        fs::write(&file, line)?;
        // sshd's StrictModes: root's, not writable by others.
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644))?;
        Ok(Self { file })
    }
}

impl Drop for ShortLivedKey {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.file);
    }
}

/// The arguments of the remote restore, parsed like the command line so every
/// default stays the CLI's.
fn restore_args(
    args: &RescueArgs,
    repo: &Path,
    from_backup: Option<&str>,
    keys: &KeyFiles,
) -> Result<RestoreArgs> {
    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        restore: RestoreArgs,
    }
    let mut argv: Vec<String> = vec![
        "neo-restore".into(),
        repo.display().to_string(),
        "--host".into(),
        args.host.clone(),
        "--key".into(),
        keys.admin.display().to_string(),
        "--backup-key".into(),
        keys.backup.display().to_string(),
        // The repo here is a copy: the restored one keeps the backup's remote.
        format!("--origin={}", remote_url(repo)),
        "--yes".into(),
    ];
    if let Some(spec) = from_backup {
        argv.extend(["--from-backup".into(), spec.to_string()]);
    }
    if let Some(d) = &args.disk {
        argv.extend(["--disk".into(), d.clone()]);
    }
    if args.check {
        argv.push("--check".into());
    } else {
        argv.push("--reboot".into());
    }
    Ok(Cli::try_parse_from(argv)
        .context("restore arguments")?
        .restore)
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(check: bool) -> RescueArgs {
        RescueArgs {
            client: "brugg".into(),
            host: "root@203.0.113.7".into(),
            disk: Some("/dev/vda".into()),
            key_file: "/run/neo-web/k".into(),
            passphrase_file: None,
            folder: None,
            check,
            op: None,
        }
    }

    #[test]
    fn restore_gets_the_keys_and_the_store() {
        let keys = KeyFiles {
            admin: "/run/r/admin_key".into(),
            backup: "/run/r/backup_key".into(),
        };
        let r = restore_args(
            &args(false),
            Path::new("/run/r/config"),
            Some("https://brugg@backup.example.com/backups"),
            &keys,
        )
        .unwrap();
        assert_eq!(r.repo.as_deref(), Some("/run/r/config"));
        assert_eq!(r.host.as_deref(), Some("root@203.0.113.7"));
        assert_eq!(r.key.as_deref(), Some(Path::new("/run/r/admin_key")));
        assert_eq!(
            r.backup_key.as_deref(),
            Some(Path::new("/run/r/backup_key"))
        );
        assert_eq!(
            r.from_backup.as_deref(),
            Some("https://brugg@backup.example.com/backups")
        );
        assert_eq!(r.disk.as_deref(), Some("/dev/vda"));
        assert!(r.yes && r.reboot && !r.check);
        let c = restore_args(&args(true), Path::new("/x"), None, &keys).unwrap();
        assert!(c.check && !c.reboot && c.from_backup.is_none());
        std::mem::forget(keys);
    }

    #[test]
    fn shred_removes() {
        let t = TempDir::new("neo-rescue-test").unwrap();
        let f = t.join("k");
        fs::write(&f, "secret").unwrap();
        shred(&f);
        assert!(!f.exists());
        shred(&f);
    }
}
