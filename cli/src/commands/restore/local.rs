//! Local mode: as root on the machine being restored, booted from a NixOS
//! installer (docs/design/backup-restore.md §4).

use anyhow::{bail, Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use toml_edit::DocumentMut;

use super::backup::{self, Access, Format, Source};
use super::{
    access_tokens, int_setting, mount_zfs, restore_identity, under, DataSource, RestoreArgs,
    DEFAULT_CONFIG_PATH,
};
use crate::utils::command::{confirm, is_root};
use crate::utils::execute_command;
use crate::utils::settings::{disko_enabled, str_at};

pub const WORK_DIR: &str = "/root/neo-restore";

fn capture(cmd: &mut Command) -> Result<String> {
    let out = cmd
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("run {:?}", cmd.get_program()))?;
    if !out.status.success() {
        bail!("{:?} failed", cmd.get_program());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
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

fn git(repo: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C")
        .arg(repo)
        .args(["-c", "safe.directory=*", "-c", "user.name=neo restore"])
        .args(["-c", "user.email=neo@localhost"]);
    c
}

/// `git+file:` / `path:` plugins that do not exist on this machine.
pub(crate) fn missing_local_plugins(doc: &DocumentMut) -> Vec<String> {
    crate::utils::plugins::plugins_from_doc(doc)
        .into_iter()
        .map(|p| p.url)
        .filter_map(|url| {
            let base = url.split(['?', '#']).next().unwrap_or(&url);
            let dir = ["git+file://", "git+file:", "path:"]
                .iter()
                .find_map(|p| base.strip_prefix(p))
                .map(str::to_string)?;
            Some((url, dir))
        })
        .filter(|(_, p)| !Path::new(p).exists())
        .map(|(u, _)| u)
        .collect()
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

    // 0. --from-backup: the backup to restore from, and the config repo in it
    //    when no repo is given (the admin key opens an encrypted backup).
    let key = args.key.as_deref();
    // Opens neo-backup.age (age reads ~/.ssh/id_ed25519 only when told to).
    let admin_key = args.key.clone().or_else(super::remote::default_key);
    let from_backup = match &args.from_backup {
        Some(spec) => {
            let access = Access {
                identities: args
                    .backup_key
                    .iter()
                    .chain(admin_key.iter())
                    .cloned()
                    .collect(),
                known_hosts: work.join("known_hosts"),
                work: work.join("backup"),
            };
            let source = backup::detect(&access, Source::from_location(spec)?)?;
            let repo = if args.repo.is_none() {
                let out = work.join("from-backup");
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
            Some((source, repo))
        }
        None => None,
    };
    let repo_url = match from_backup.as_ref().and_then(|(_, r)| r.as_ref()) {
        Some(r) => r.display().to_string(),
        None => args
            .repo
            .clone()
            .context("give the config repo, or --from-backup <backup>")?,
    };
    // Remote of the installed repo: the backup's copy keeps its own.
    let origin = match (
        &args.origin,
        from_backup.as_ref().and_then(|(_, r)| r.as_ref()),
    ) {
        (Some(o), _) => o.clone(),
        (None, Some(r)) => remote_url(r),
        (None, None) => repo_url.clone(),
    };

    // 1. Clone (HEAD only: uncommitted changes in a source checkout are not restored).
    let staging = work.join("config");
    let _ = fs::remove_dir_all(&staging);
    println!("Cloning {repo_url} …");
    execute_command(
        Command::new("git")
            .args([
                "-c",
                "safe.directory=*",
                "clone",
                "--no-hardlinks",
                &repo_url,
            ])
            .arg(&staging),
    )?;
    let settings_path = staging.join("settings.toml");
    let mut doc: DocumentMut = fs::read_to_string(&settings_path)
        .context("the repository has no settings.toml")?
        .parse()
        .context("parse settings.toml")?;

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

    // 2. Adapt the repo to this machine.
    let mut changes = Vec::new();
    if let Some(disk) = &args.disk {
        doc["disko"]["mainDisk"] = toml_edit::value(disk.as_str());
        fs::write(&settings_path, doc.to_string())?;
        changes.push(format!("disko.mainDisk = {disk}"));
    }
    if !args.keep_hardware {
        let hw = capture(
            Command::new("nixos-generate-config")
                .args(["--no-filesystems", "--show-hardware-config"]),
        )
        .context("nixos-generate-config (pass --keep-hardware to keep the repo's)")?;
        let path = staging.join("hardware-configuration.nix");
        if fs::read_to_string(&path).unwrap_or_default().trim() != hw.trim() {
            fs::write(&path, format!("{hw}\n"))?;
            changes.push("hardware-configuration.nix for this machine".into());
        }
    }
    if !changes.is_empty() {
        execute_command(git(&staging).args(["add", "-A"]))?;
        execute_command(git(&staging).args([
            "commit",
            "-q",
            "-m",
            &format!("restore: {}", changes.join(", ")),
        ]))?;
    }

    // 3. Unlock: escrowed host key and the tokens for private plugins.
    let identity_target = work.join("identity");
    let _ = fs::remove_dir_all(&identity_target);
    let identity = restore_identity(&staging, &identity_target, &doc, key)?;
    let host_key = identity_target.join("etc/ssh/ssh_host_ed25519_key");
    let tokens = access_tokens(&staging, &doc, key)?;
    let nix_config = {
        let mut c = "experimental-features = nix-command flakes\n".to_string();
        if let Some(t) = &tokens {
            c.push_str(t);
            c.push('\n');
        }
        c
    };

    // 4. Preflight.
    let missing = missing_local_plugins(&doc);
    if !missing.is_empty() {
        bail!(
            "local plugins are not on this machine: {}. Put them into git, or copy them to the same path here first",
            missing.join(", ")
        );
    }
    let main_disk = str_at(&doc, &["disko", "mainDisk"])
        .unwrap_or("/dev/vda")
        .to_string();
    if !mounted && !Path::new(&main_disk).exists() {
        bail!("disko.mainDisk {main_disk} does not exist here; pick one from `ls -l /dev/disk/by-id` and pass --disk");
    }
    let root = backup::volumes_root(&doc);
    let pool = str_at(&doc, &["disko", "poolName"])
        .unwrap_or("zroot")
        .to_string();
    let config_path = str_at(&doc, &["neo-cli", "server", "configPath"])
        .unwrap_or(DEFAULT_CONFIG_PATH)
        .to_string();
    let config_rel = config_path
        .strip_prefix(&format!("{root}/"))
        .map(str::to_string);

    // Every backup destination, in the order they are tried.
    let with_data = args.data == DataSource::Backup;
    let sources: Vec<Source> = match (&from_backup, with_data) {
        (_, false) => Vec::new(),
        (Some((s, _)), true) => vec![s.clone()],
        (None, true) => Source::from_settings(&doc)?,
    };
    if with_data && sources.is_empty() && args.data_from.is_none() {
        bail!("the backup is off in this config: restore without data (--data none), or point --data-from at a copy");
    }
    let mut identities = Vec::new();
    if let Some(k) = &args.backup_key {
        identities.push(k.clone());
    }
    if identity && host_key.exists() {
        identities.push(host_key.clone());
    }
    if let Some(k) = &admin_key {
        identities.push(k.clone());
    }
    let access = Access {
        identities,
        known_hosts: work.join("known_hosts"),
        work: work.join("backup"),
    };
    // The first destination that can be read; the ones after it are the
    // fallbacks if copying from it fails.
    let usable = |s: &Source| -> Result<String> {
        if s.format == Format::Zfs && (mounted || !disko_on) {
            bail!("a ZFS backup restores into the Disko layout only");
        }
        backup::check(&access, s, args.data_from.as_deref())
    };
    let mut first = None;
    let mut unreadable = Vec::new();
    for (i, s) in sources.iter().enumerate() {
        match usable(s) {
            Ok(line) => {
                first = Some((i, line));
                break;
            }
            Err(e) => {
                println!("✗ cannot read the backup {}: {e:#}", s.describe());
                unreadable.push(format!("{}: {e:#}", s.describe()));
            }
        }
    }
    let data_line = match (&first, sources.is_empty()) {
        (_, true) => "none (empty data)".into(),
        (None, false) => bail!(
            "no backup destination can be read: {}",
            unreadable.join("; ")
        ),
        (Some((i, line)), false) => {
            let rest = sources.len() - i - 1;
            format!(
                "{} — {line}{}",
                sources[*i].describe(),
                match rest {
                    0 => String::new(),
                    n => format!(" (then {n} more if it fails)"),
                }
            )
        }
    };
    if !sources.is_empty() && config_rel.is_none() {
        bail!("the config repo ({config_path}) is not inside {root}");
    }

    let flake = format!("{}#nixosConfigurations.{}", staging.display(), args.config);
    let disko_script = if mounted {
        None
    } else {
        println!("Evaluating the configuration (first run builds a lot) …");
        Some(
            capture(
                Command::new("nix")
                    .args(["build", "--no-link", "--print-out-paths"])
                    .arg(format!("{flake}.config.system.build.diskoScript"))
                    .env("NIX_CONFIG", &nix_config),
            )
            .context("the configuration does not evaluate")?,
        )
    };

    println!();
    println!("Restore plan");
    println!(
        "  machine   {} (domain {})",
        backup::hostname(&doc),
        str_at(&doc, &["services", "swag", "domain"]).unwrap_or("-")
    );
    if mounted {
        println!(
            "  disk      already mounted at {} (no Disko)",
            args.target.display()
        );
    } else {
        println!("  disk      {main_disk} — ERASED");
    }
    println!(
        "  identity  {}",
        if identity {
            "old SSH host key (secrets decrypt)"
        } else {
            "new host key"
        }
    );
    println!("  data      {data_line}");
    println!(
        "  install   {}{}",
        if args.no_install {
            "skipped"
        } else {
            "nixos-install"
        },
        if args.reboot { ", then reboot" } else { "" }
    );
    println!();
    if args.check {
        let _ = fs::remove_dir_all(&identity_target);
        return Ok(());
    }
    if !args.yes {
        confirm(&format!("This erases {main_disk}."))?;
    }

    // 5. Disko.
    cap_arc();
    if let Some(script) = disko_script {
        if is_mountpoint(&args.target) {
            let _ = Command::new("umount").arg("-R").arg(&args.target).status();
        }
        // The last attempt's pool (not a backup pool on an attached disk).
        let _ = Command::new("zpool").args(["export", &pool]).status();
        execute_command(Command::new(format!("{script}")).arg("--yes-wipe-all-disks"))?;
        cap_arc();
    }

    // 6. Data, from the next destination when one fails.
    if let (Some((first, _)), Some(rel)) = (&first, &config_rel) {
        let data_root = under(&args.target, &root);
        let dataset = format!("{pool}/neo");
        fs::create_dir_all(&data_root)?;
        let mut errors = Vec::new();
        let mut restored = false;
        for (i, s) in sources.iter().enumerate().skip(*first) {
            if i != *first {
                if let Err(e) = usable(s) {
                    println!("✗ cannot read the backup {}: {e:#}", s.describe());
                    errors.push(format!("{}: {e:#}", s.describe()));
                    continue;
                }
                println!("→ trying the next backup: {}", s.describe());
                reset_data_root(&data_root, disko_on.then_some(dataset.as_str()))?;
            }
            match backup::restore_into(
                &access,
                s,
                args.data_from.as_deref(),
                &data_root,
                rel,
                &dataset,
                &root,
            ) {
                Ok(()) => {
                    restored = true;
                    break;
                }
                Err(e) => {
                    println!("✗ restoring from {} failed: {e:#}", s.describe());
                    errors.push(format!("{}: {e:#}", s.describe()));
                }
            }
        }
        if !restored {
            bail!(
                "no backup destination could be restored: {}",
                errors.join("; ")
            );
        }
    }

    // 7. Repo, host key, install.
    let repo = under(&args.target, &config_path);
    if repo.exists() && fs::read_dir(&repo)?.next().is_some() {
        bail!("{} already exists and is not empty", repo.display());
    }
    if let Some(parent) = repo.parent() {
        fs::create_dir_all(parent)?;
    }
    execute_command(
        Command::new("git")
            .args(["-c", "safe.directory=*", "clone", "-q", "--no-hardlinks"])
            .arg(&staging)
            .arg(&repo),
    )?;
    if origin.is_empty() || Path::new(&origin).is_dir() {
        execute_command(git(&repo).args(["remote", "remove", "origin"]))?;
    } else {
        execute_command(git(&repo).args(["remote", "set-url", "origin", &origin]))?;
    }
    if restore_identity(&repo, &args.target, &doc, key)? {
        println!(
            "✓ Host key restored from the escrow: same identity, secrets decrypt on first boot"
        );
    } else {
        println!("! No identity escrow: the machine gets a new host key (re-trust it on clients)");
    }
    let uid = int_setting(&doc, "uid", 1000);
    let gid = int_setting(&doc, "gid", 1000);
    execute_command(
        Command::new("chown")
            .arg("-R")
            .arg(format!("{uid}:{gid}"))
            .arg(&repo),
    )?;
    let _ = fs::remove_dir_all(&identity_target);
    let _ = fs::remove_dir_all(work.join("backup"));

    if args.no_install {
        println!(
            "Config repository at {} (skipped nixos-install)",
            repo.display()
        );
        return finish(args);
    }
    // From the staging clone: the same commit, but owned by root. Nix refuses
    // the installed repo (owned by the neo user) as root ("not owned by
    // current user").
    execute_command(
        Command::new("nixos-install")
            .arg("--root")
            .arg(&args.target)
            .arg("--no-root-passwd")
            .arg("--flake")
            .arg(format!("{}#{}", staging.display(), args.config))
            .env("NIX_CONFIG", &nix_config),
    )?;

    // 8. Boot entry (UEFI): Neo's boot loader install puts NixOS first.
    if Path::new("/sys/firmware/efi/efivars").exists() {
        if let Ok(out) = capture(&mut Command::new("efibootmgr")) {
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
    }
    println!("Installed {}.", backup::hostname(&doc));
    finish(args)
}

/// Commit what the backup's config repo had not committed yet (settings
/// saved in the web UI before the last backup), so the clone carries it.
pub(crate) fn commit_uncommitted(repo: &Path) -> Result<()> {
    if !repo.join(".git").exists() {
        bail!("{} in the backup is not a git repository", repo.display());
    }
    let status = capture(git(repo).args(["status", "--porcelain"]))?;
    if status.is_empty() {
        return Ok(());
    }
    println!("  committing changes the backup's repo had not committed yet");
    execute_command(git(repo).args(["add", "-A"]))?;
    execute_command(git(repo).args([
        "commit",
        "-q",
        "-m",
        "restore: uncommitted changes from the backup",
    ]))
}

/// `origin` of a repo, "" without one.
pub(crate) fn remote_url(repo: &Path) -> String {
    git(repo)
        .args(["remote", "get-url", "origin"])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn finish(args: &RestoreArgs) -> Result<()> {
    if args.shred_key {
        if let Some(k) = &args.key {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_missing_local_plugins() {
        let doc: DocumentMut = "[core]\nplugins = [\"github:a/b\", \"git+file:/nonexistent/x\", \"path:/\", \"git+file:///nope?ref=main\"]\n"
            .parse()
            .unwrap();
        assert_eq!(
            missing_local_plugins(&doc),
            vec!["git+file:/nonexistent/x", "git+file:///nope?ref=main"]
        );
    }
}
