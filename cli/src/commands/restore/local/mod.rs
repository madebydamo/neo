//! Local mode: as root on the machine being restored, booted from a NixOS
//! installer (docs/design/backup-restore.md §4).

use anyhow::{bail, Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use toml_edit::DocumentMut;

use super::{access_tokens, int_setting, restore_identity, under, RestoreArgs};
use crate::utils::backup::config_repo::{commit_uncommitted, git, remote_url};
use crate::utils::backup::{self, default_key, mount_zfs, Access, RestoreInto, Source};
use crate::utils::command::execute_command;
use crate::utils::command::{confirm, is_root, stdout_checked};
use crate::utils::settings::{self, disko_enabled, FILE as SETTINGS_FILE};

mod preflight;
mod repo;

use preflight::{preflight, Plan};
pub(crate) use repo::{default_origin, stage_repo};

pub const WORK_DIR: &str = "/root/neo-restore";

/// Folders under [`WORK_DIR`] the restore works in.
struct Dirs {
    work: PathBuf,
    /// Clone of the config repo, adapted to this machine.
    staging: PathBuf,
    /// The escrowed host key, unpacked to open the backup.
    identity: PathBuf,
}

/// `--from-backup`: the backup, and the config repo taken out of it when no
/// repo was given.
struct FromBackup {
    source: Source,
    repo: Option<PathBuf>,
}

/// The staged config repo's settings and how the target disk is handled.
struct Staged {
    doc: DocumentMut,
    disko_on: bool,
    /// Target already mounted by hand (no Disko).
    mounted: bool,
}

/// What unlocking the repo found.
struct Unlocked {
    /// The escrowed host key was restored.
    identity: bool,
    host_key: PathBuf,
    /// `NIX_CONFIG` for builds (flakes plus tokens for private plugins).
    nix_config: String,
}

fn is_mountpoint(p: &Path) -> bool {
    Command::new("findmnt")
        .arg("-n")
        .arg("--mountpoint")
        .arg(p)
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Empty the data folder after a failed copy, before the next destination.
/// With Disko it is the data dataset's mountpoint, mounted again if a failed
/// ZFS receive left it unmounted.
fn reset_data_root(data_root: &Path, dataset: Option<&str>) -> Result<()> {
    if let (Some(ds), false) = (dataset, is_mountpoint(data_root)) {
        mount_zfs(ds, data_root)?;
    }
    for entry in fs::read_dir(data_root)? {
        let path = entry?.path();
        if path.is_dir() && !path.is_symlink() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Cap the ZFS ARC: the installer's Nix store is a tmpfs in the same RAM, and
/// a filling ARC made nix die with SIGBUS on the real move.
fn cap_arc() {
    let param = Path::new("/sys/module/zfs/parameters/zfs_arc_max");
    let Ok(meminfo) = fs::read_to_string("/proc/meminfo") else {
        return;
    };
    let kb: u64 = meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|v| v.split_whitespace().next()?.parse().ok())
        .unwrap_or(0);
    if param.exists() && kb > 0 {
        let _ = fs::write(param, (kb * 1024 / 4).to_string());
    }
}

pub fn run(args: &RestoreArgs) -> Result<()> {
    if !is_root() {
        bail!("run neo restore as root on the machine to restore, or pass --host root@<machine> from another computer");
    }
    if !args.target.is_dir() {
        fs::create_dir_all(&args.target)?;
    }
    let work = PathBuf::from(WORK_DIR);
    fs::create_dir_all(&work)?;
    fs::set_permissions(&work, fs::Permissions::from_mode(0o700))?;
    let dirs = Dirs {
        staging: work.join("config"),
        identity: work.join("identity"),
        work,
    };
    // Opens neo-backup.age (age reads ~/.ssh/id_ed25519 only when told to).
    let admin_key = args.key.clone().or_else(default_key);

    let from_backup = open_from_backup(args, &dirs, admin_key.as_ref())?;
    let (repo_url, origin) = repo_and_origin(args, from_backup.as_ref())?;
    let staged = stage_and_adapt(args, &dirs, &repo_url)?;
    let unlocked = unlock(&dirs, &staged.doc, args.key.as_deref())?;
    let plan = preflight(
        args,
        &dirs,
        &staged,
        &unlocked,
        from_backup.as_ref(),
        admin_key.as_ref(),
    )?;
    plan.print(args, &staged.doc, unlocked.identity);
    if args.check {
        let _ = fs::remove_dir_all(&dirs.identity);
        return Ok(());
    }
    if !args.yes {
        confirm(&format!("This erases {}.", plan.main_disk))?;
    }

    partition(args, &plan)?;
    restore_data(args, &plan)?;
    let repo = install_repo(args, &dirs, &staged.doc, &plan, &origin)?;
    if args.no_install {
        println!(
            "Config repository at {} (skipped nixos-install)",
            repo.display()
        );
        return finish(args);
    }
    install(args, &dirs, &unlocked)?;
    boot_entry();
    println!("Installed {}.", backup::hostname(&staged.doc));
    finish(args)
}

/// Step 0, `--from-backup`: the backup to restore from, and the config repo
/// in it when no repo is given (the admin key opens an encrypted backup).
fn open_from_backup(
    args: &RestoreArgs,
    dirs: &Dirs,
    admin_key: Option<&PathBuf>,
) -> Result<Option<FromBackup>> {
    let Some(spec) = &args.from_backup else {
        return Ok(None);
    };
    let access = Access {
        identities: args.backup_key.iter().chain(admin_key).cloned().collect(),
        known_hosts: dirs.work.join("known_hosts"),
        work: dirs.work.join("backup"),
    };
    let source = backup::detect(&access, Source::from_location(spec)?)?;
    let repo = if args.repo.is_none() {
        let out = dirs.work.join("from-backup");
        let _ = fs::remove_dir_all(&out);
        println!(
            "Taking the config repo out of the backup ({}) …",
            source.describe()
        );
        backup::extract_config(&access, &source, &out)?;
        commit_uncommitted(&out)?;
        Some(out)
    } else {
        None
    };
    Ok(Some(FromBackup { source, repo }))
}

/// The repo to clone and the remote of the installed copy (the backup's copy
/// keeps its own).
fn repo_and_origin(
    args: &RestoreArgs,
    from_backup: Option<&FromBackup>,
) -> Result<(String, String)> {
    let backup_repo = from_backup.and_then(|fb| fb.repo.as_ref());
    let repo_url = match backup_repo {
        Some(r) => r.display().to_string(),
        None => args
            .repo
            .clone()
            .context("give the config repo, or --from-backup <backup>")?,
    };
    let origin = match (&args.origin, backup_repo) {
        (Some(o), _) => o.clone(),
        (None, Some(r)) => remote_url(r),
        (None, None) => default_origin(&repo_url),
    };
    Ok((repo_url, origin))
}

/// Steps 1–2: clone the repo (HEAD only: uncommitted changes in a source
/// checkout are not restored) and adapt it to this machine.
fn stage_and_adapt(args: &RestoreArgs, dirs: &Dirs, repo_url: &str) -> Result<Staged> {
    let staging = &dirs.staging;
    let _ = fs::remove_dir_all(staging);
    stage_repo(repo_url, staging)?;
    let settings_path = staging.join(SETTINGS_FILE);
    let mut doc = settings::load(&settings_path).context("the repository's settings.toml")?;

    let disko_on = disko_enabled(&doc);
    // With Disko the target is always made fresh (a rerun after a failure
    // finds the last attempt still mounted). Without it, what is mounted is
    // the target.
    let mounted = !disko_on && is_mountpoint(&args.target);
    if !mounted && !disko_on {
        bail!(
            "Disko is off in this config: partition the disk by hand, mount it at {}, and run neo restore again",
            args.target.display()
        );
    }

    let mut changes = Vec::new();
    if let Some(disk) = &args.disk {
        doc["disko"]["mainDisk"] = toml_edit::value(disk.as_str());
        fs::write(&settings_path, doc.to_string())?;
        changes.push(format!("disko.mainDisk = {disk}"));
    }
    if !args.keep_hardware {
        let hw = stdout_checked(
            Command::new("nixos-generate-config")
                .args(["--no-filesystems", "--show-hardware-config"]),
            "nixos-generate-config",
        )
        .context("nixos-generate-config (pass --keep-hardware to keep the repo's)")?;
        let hw = hw.trim();
        let path = staging.join("hardware-configuration.nix");
        if fs::read_to_string(&path).unwrap_or_default().trim() != hw.trim() {
            fs::write(&path, format!("{hw}\n"))?;
            changes.push("hardware-configuration.nix for this machine".into());
        }
    }
    if !changes.is_empty() {
        execute_command(git(staging).args(["add", "-A"]))?;
        execute_command(git(staging).args([
            "commit",
            "-q",
            "-m",
            &format!("restore: {}", changes.join(", ")),
        ]))?;
    }
    Ok(Staged {
        doc,
        disko_on,
        mounted,
    })
}

/// Step 3: escrowed host key and the tokens for private plugins.
fn unlock(dirs: &Dirs, doc: &DocumentMut, key: Option<&Path>) -> Result<Unlocked> {
    let _ = fs::remove_dir_all(&dirs.identity);
    let identity = restore_identity(&dirs.staging, &dirs.identity, doc, key)?;
    let host_key = dirs.identity.join("etc/ssh/ssh_host_ed25519_key");
    let tokens = access_tokens(&dirs.staging, doc, key)?;
    let mut nix_config = "experimental-features = nix-command flakes\n".to_string();
    if let Some(t) = &tokens {
        nix_config.push_str(t);
        nix_config.push('\n');
    }
    Ok(Unlocked {
        identity,
        host_key,
        nix_config,
    })
}

/// Step 5: Disko.
fn partition(args: &RestoreArgs, plan: &Plan) -> Result<()> {
    cap_arc();
    if let Some(script) = &plan.disko_script {
        if is_mountpoint(&args.target) {
            let _ = Command::new("umount").arg("-R").arg(&args.target).status();
        }
        // The last attempt's pool (not a backup pool on an attached disk).
        let _ = Command::new("zpool").args(["export", &plan.pool]).status();
        execute_command(Command::new(script).arg("--yes-wipe-all-disks"))?;
        cap_arc();
    }
    Ok(())
}

/// Step 6: data, from the next destination when one fails.
fn restore_data(args: &RestoreArgs, plan: &Plan) -> Result<()> {
    let (Some(first), Some(rel)) = (plan.first, &plan.config_rel) else {
        return Ok(());
    };
    let data_root = under(&args.target, &plan.root);
    let dataset = format!("{}/neo", plan.pool);
    fs::create_dir_all(&data_root)?;
    let mut errors = Vec::new();
    for (i, s) in plan.sources.iter().enumerate().skip(first) {
        if i != first {
            if let Err(e) = plan.usable(s, args.data_from.as_deref()) {
                println!("✗ cannot read the backup {}: {e:#}", s.describe());
                errors.push(format!("{}: {e:#}", s.describe()));
                continue;
            }
            println!("→ trying the next backup: {}", s.describe());
            reset_data_root(&data_root, plan.disko_on.then_some(dataset.as_str()))?;
        }
        let into = RestoreInto {
            data_from: args.data_from.as_deref(),
            target_root: &data_root,
            config_rel: rel,
            local_dataset: &dataset,
            root: &plan.root,
        };
        match backup::restore_into(&plan.access, s, &into) {
            Ok(()) => return Ok(()),
            Err(e) => {
                println!("✗ restoring from {} failed: {e:#}", s.describe());
                errors.push(format!("{}: {e:#}", s.describe()));
            }
        }
    }
    bail!(
        "no backup destination could be restored: {}",
        errors.join("; ")
    )
}

/// Step 7: the config repo and the host key on the target.
fn install_repo(
    args: &RestoreArgs,
    dirs: &Dirs,
    doc: &DocumentMut,
    plan: &Plan,
    origin: &str,
) -> Result<PathBuf> {
    let repo = under(&args.target, &plan.config_path);
    if repo.exists() && fs::read_dir(&repo)?.next().is_some() {
        bail!("{} already exists and is not empty", repo.display());
    }
    if let Some(parent) = repo.parent() {
        fs::create_dir_all(parent)?;
    }
    execute_command(
        Command::new("git")
            .args(["-c", "safe.directory=*", "clone", "-q", "--no-hardlinks"])
            .arg(&dirs.staging)
            .arg(&repo),
    )?;
    if origin.is_empty() || Path::new(origin).is_dir() {
        execute_command(git(&repo).args(["remote", "remove", "origin"]))?;
    } else {
        execute_command(git(&repo).args(["remote", "set-url", "origin", origin]))?;
    }
    if restore_identity(&repo, &args.target, doc, args.key.as_deref())? {
        println!(
            "✓ Host key restored from the escrow: same identity, secrets decrypt on first boot"
        );
    } else {
        println!("! No identity escrow: the machine gets a new host key (re-trust it on clients)");
    }
    let uid = int_setting(doc, "uid", 1000);
    let gid = int_setting(doc, "gid", 1000);
    execute_command(
        Command::new("chown")
            .arg("-R")
            .arg(format!("{uid}:{gid}"))
            .arg(&repo),
    )?;
    let _ = fs::remove_dir_all(&dirs.identity);
    let _ = fs::remove_dir_all(dirs.work.join("backup"));
    Ok(repo)
}

/// Step 7, continued: `nixos-install`.
fn install(args: &RestoreArgs, dirs: &Dirs, unlocked: &Unlocked) -> Result<()> {
    // From the staging clone: the same commit, but owned by root. Nix refuses
    // the installed repo (owned by the neo user) as root ("not owned by
    // current user").
    execute_command(
        Command::new("nixos-install")
            .arg("--root")
            .arg(&args.target)
            .arg("--no-root-passwd")
            .arg("--flake")
            .arg(format!("{}#{}", dirs.staging.display(), args.config))
            .env("NIX_CONFIG", &unlocked.nix_config),
    )
}

/// Step 8, boot entry (UEFI): Neo's boot loader install puts NixOS first.
fn boot_entry() {
    if !Path::new("/sys/firmware/efi/efivars").exists() {
        return;
    }
    let Ok(out) = stdout_checked(&mut Command::new("efibootmgr"), "efibootmgr") else {
        return;
    };
    let first = out
        .lines()
        .find_map(|l| l.strip_prefix("BootOrder: "))
        .and_then(|o| o.split(',').next())
        .unwrap_or("");
    let ours = out
        .lines()
        .any(|l| l.starts_with(&format!("Boot{first}")) && l.contains("NixOS"));
    if ours {
        println!("✓ Firmware boots NixOS first (Boot{first})");
    } else {
        println!("! NixOS is not first in the firmware boot order:\n{out}");
    }
}

fn finish(args: &RestoreArgs) -> Result<()> {
    if args.shred_key {
        for k in args.key.iter().chain(&args.backup_key) {
            let _ = fs::remove_file(k);
        }
    }
    if args.reboot {
        println!("Rebooting …");
        let _ = Command::new("sync").status();
        let _ = Command::new("umount").arg("-R").arg(&args.target).status();
        let _ = Command::new("zpool").args(["export", "-a"]).status();
        execute_command(Command::new("systemd-run").args([
            "--on-active=3",
            "systemctl",
            "reboot",
        ]))?;
    } else {
        println!(
            "Reboot when ready (umount -R {} && zpool export -a && reboot).",
            args.target.display()
        );
    }
    Ok(())
}
