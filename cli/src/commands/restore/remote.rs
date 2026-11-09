//! Remote mode: `neo restore <repo> --host root@IP` from a client. Brings the
//! target into a NixOS installer (nixos-anywhere kexec) when it is not one,
//! copies the repo, the admin key and this neo into its RAM and runs the local
//! mode there as a systemd unit, streaming its journal back.

use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use super::backup::{self, Access, Source};
use super::local::{commit_uncommitted, default_origin, remote_url, stage_repo, WORK_DIR};
use super::{DataSource, RestoreArgs, DEFAULT_TARGET};
use crate::utils::command::{confirm, shell_quote, stdout_checked};
use crate::utils::execute_command;
use crate::utils::fsutil::TempDir;

const UNIT: &str = "neo-restore";

/// `root@IP:PORT` → (`root@IP`, Some(PORT)). IPv6 needs brackets: `root@[::1]:22`.
pub(crate) fn split_port(host: &str) -> (String, Option<String>) {
    if let Some((h, p)) = host.rsplit_once(':') {
        let bracketed = h.contains('[') && h.ends_with(']');
        if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && (bracketed || !h.contains(':'))
        {
            return (h.replace(['[', ']'], ""), Some(p.to_string()));
        }
    }
    (host.to_string(), None)
}
const RESULT_MARK: &str = "NEO-RESTORE-RESULT=";
/// The keys in the installer's work folder.
const ADMIN_KEY: &str = "admin-key";
const BACKUP_KEY: &str = "backup-key";

impl RestoreArgs {
    /// `neo restore` arguments for the target (not yet shell-quoted), the same for
    /// its preflight and its run. `run` adds what only applies to the run. A new
    /// field fails to compile here until it is forwarded or marked client-only.
    fn forwarded_argv(&self, origin: &str) -> Vec<String> {
        let RestoreArgs {
            // Replaced: copied into the installer's work folder.
            repo: _,
            key: _,
            // Client-only.
            host: _,
            neo: _,
            // Added per stage: --check for the preflight; --yes, --shred-key and
            // --reboot for the run.
            check: _,
            yes: _,
            reboot: _,
            shred_key: _,
            // Passed on as the resolved `origin` argument.
            origin: _,
            from_backup,
            disk,
            data,
            data_from,
            backup_key,
            keep_hardware,
            target,
            config,
            no_install,
        } = self;
        let mut argv: Vec<String> = vec![
            "restore".into(),
            format!("{WORK_DIR}/repo"),
            "--origin".into(),
            origin.into(),
            "--key".into(),
            format!("{WORK_DIR}/{ADMIN_KEY}"),
            "--config".into(),
            config.clone(),
        ];
        let mut opt = |flag: &str, value: Option<String>| {
            if let Some(v) = value {
                argv.extend([flag.to_string(), v]);
            }
        };
        opt("--disk", disk.clone());
        opt(
            "--data-from",
            data_from.as_ref().map(|p| p.display().to_string()),
        );
        opt("--from-backup", from_backup.clone());
        opt(
            "--backup-key",
            backup_key
                .as_ref()
                .map(|_| format!("{WORK_DIR}/{BACKUP_KEY}")),
        );
        opt(
            "--target",
            (target.as_os_str() != DEFAULT_TARGET).then(|| target.display().to_string()),
        );
        if *data == DataSource::None {
            argv.extend(["--data".to_string(), "none".to_string()]);
        }
        if *keep_hardware {
            argv.push("--keep-hardware".into());
        }
        if *no_install {
            argv.push("--no-install".into());
        }
        argv
    }
}

/// A NixOS installer: root on tmpfs (or the ISO's overlay) and nixos-install.
const INSTALLER_TEST: &str = "test -e /etc/NIXOS && command -v nixos-install >/dev/null && case $(findmnt -no FSTYPE /) in tmpfs|overlay|rootfs) true;; *) false;; esac";

/// The machine being restored and how to log in to it.
struct Target {
    host: String,
    port: Option<String>,
    /// The admin key also logs in (besides the agent and ssh's defaults).
    key: PathBuf,
}

impl Target {
    fn new(spec: &str, key: PathBuf) -> Self {
        let (host, port) = split_port(spec);
        Self { host, port, key }
    }

    /// ssh options. `ephemeral`: the installer's host key changes with every
    /// kexec, so it is never written to known_hosts.
    fn ssh_opts(&self, ephemeral: bool) -> Vec<String> {
        let mut o: Vec<String> = [
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ServerAliveInterval=15",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        if ephemeral {
            o.extend(
                [
                    "-o",
                    "StrictHostKeyChecking=no",
                    "-o",
                    "UserKnownHostsFile=/dev/null",
                    "-o",
                    "LogLevel=ERROR",
                ]
                .iter()
                .map(|s| s.to_string()),
            );
        } else {
            o.extend([
                "-o".to_string(),
                "StrictHostKeyChecking=accept-new".to_string(),
            ]);
        }
        if let Some(p) = &self.port {
            o.extend(["-p".to_string(), p.clone()]);
        }
        o.extend(["-i".to_string(), self.key.display().to_string()]);
        o
    }

    fn ssh(&self, ephemeral: bool, script: &str) -> Command {
        let mut c = Command::new("ssh");
        c.args(self.ssh_opts(ephemeral)).arg(&self.host).arg(script);
        c
    }

    fn ssh_ok(&self, ephemeral: bool, script: &str) -> bool {
        self.ssh(ephemeral, script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// `nix copy` / `nix flake archive` to the installer's store.
    fn nix_to_store(&self, features: &str, args: &[&str]) -> Command {
        let mut c = Command::new("nix");
        c.args(["--extra-experimental-features", features])
            .args(args)
            .args(["--no-check-sigs", "--to"])
            .arg(format!("ssh-ng://{}", self.host))
            .env("NIX_SSHOPTS", self.ssh_opts(true).join(" "));
        c
    }

    fn kexec(&self) -> Result<()> {
        // nixos-anywhere wants a system to install even for the kexec phase alone:
        // hand it a placeholder store path.
        let tmp = TempDir::new("neo-restore-fake")?;
        fs::write(tmp.join("system"), "x86_64-linux")?;
        let out = stdout_checked(
            Command::new("nix")
                .args([
                    "--extra-experimental-features",
                    "nix-command",
                    "store",
                    "add-path",
                ])
                .arg(tmp.path())
                .args(["--name", "neo-restore-placeholder"]),
            "nix store add-path",
        )?;
        let p = out.trim().to_string();
        let mut anywhere = Command::new("nixos-anywhere");
        anywhere.args([
            "--phases",
            "kexec",
            "--store-paths",
            &p,
            &p,
            "--target-host",
            &self.host,
        ]);
        if let Some(port) = &self.port {
            anywhere.args(["--ssh-port", port]);
        }
        // Not `-i`: that ssh-adds the key to the user's agent (a gpg-agent then
        // asks for a passphrase to store it).
        anywhere
            .arg("--ssh-option")
            .arg(format!("IdentityFile={}", self.key.display()));
        execute_command(&mut anywhere)?;
        println!("Waiting for the installer …");
        let start = Instant::now();
        while !self.ssh_ok(true, INSTALLER_TEST) {
            if start.elapsed() > Duration::from_secs(600) {
                bail!("{} did not come back as a NixOS installer", self.host);
            }
            sleep(Duration::from_secs(5));
        }
        Ok(())
    }

    /// An installer already? Otherwise kexec into one (the disk is not touched).
    fn ensure_installer(&self, yes: bool) -> Result<()> {
        if self.ssh_ok(true, INSTALLER_TEST) {
            return Ok(());
        }
        let host = &self.host;
        if !self.ssh_ok(false, "true") {
            bail!("cannot log in to {host} as root with your SSH key");
        }
        println!(
            "{host} is not a NixOS installer: nixos-anywhere will boot one into its RAM (kexec)."
        );
        if !yes {
            confirm("The running system on it stops.")?;
        }
        self.kexec()
    }

    /// Where the running neo comes from: the wrapper's store path, copied to the
    /// target, or a flake ref run there.
    fn neo(&self, args: &RestoreArgs, repo: &Path) -> Result<String> {
        if let Some(r) = &args.neo {
            return Ok(format!(
                "nix --extra-experimental-features 'nix-command flakes' run {}#neo --",
                shell_quote(r)
            ));
        }
        if let Ok(pkg) = env::var("NEO_PACKAGE") {
            if pkg.starts_with("/nix/store/") && Path::new(&pkg).exists() {
                println!("Copying this neo to {} …", self.host);
                let copied = self
                    .nix_to_store("nix-command", &["copy"])
                    .arg(&pkg)
                    .status()
                    .context("nix copy")?;
                if copied.success() {
                    return Ok(format!("{pkg}/bin/neo"));
                }
                println!("! nix copy failed, running the neo pinned in the repo instead");
            }
        }
        Ok(format!(
            "nix --extra-experimental-features 'nix-command flakes' run {}#neo --",
            shell_quote(&pinned_neo(repo))
        ))
    }

    /// Repo and admin key into the installer's RAM.
    fn copy_repo_and_key(&self, staged: &Path) -> Result<()> {
        let host = &self.host;
        println!("Copying the config repo and your key into the installer's RAM …");
        let mut tar = Command::new("tar")
            .arg("-C")
            .arg(staged)
            .args(["-cf", "-", "config"])
            .stdout(Stdio::piped())
            .spawn()
            .context("tar")?;
        let unpack = self.ssh(
            true,
            &format!("set -e; rm -rf {WORK_DIR}/repo {WORK_DIR}/config; mkdir -p {WORK_DIR}; chmod 700 {WORK_DIR}; tar -C {WORK_DIR} -xf -; mv {WORK_DIR}/config {WORK_DIR}/repo; chown -R root:root {WORK_DIR}/repo"),
        )
        .stdin(tar.stdout.take().context("tar stdout")?)
        .status()?;
        if !tar.wait()?.success() || !unpack.success() {
            bail!("copying the repo to {host} failed");
        }
        self.copy_private_file(&self.key, ADMIN_KEY)
            .with_context(|| format!("copying the key to {host} failed"))
    }

    /// A key file into the installer's work folder, readable by root only.
    fn copy_private_file(&self, from: &Path, name: &str) -> Result<()> {
        let file = fs::File::open(from).with_context(|| format!("open {}", from.display()))?;
        if !self
            .ssh(true, &format!("umask 077; cat > {WORK_DIR}/{name}"))
            .stdin(file)
            .status()?
            .success()
        {
            bail!("writing {name} failed");
        }
        Ok(())
    }

    /// Keys out of the installer's RAM when no restore follows. A started
    /// restore removes them itself (--shred-key).
    fn remove_keys(&self) {
        self.ssh_ok(
            true,
            &format!("rm -f {WORK_DIR}/{ADMIN_KEY} {WORK_DIR}/{BACKUP_KEY}"),
        );
    }

    /// Every locked input (nixpkgs, neo, plugins) into the installer's store:
    /// fetched here with this computer's credentials, so private plugins need
    /// no tokens there and the target does not download them again.
    fn copy_inputs(&self, repo: &Path) -> Result<()> {
        println!("Copying the config's inputs to {} …", self.host);
        let archived = self
            .nix_to_store("nix-command flakes", &["flake", "archive"])
            .arg(repo)
            .status()
            .context("nix flake archive")?;
        if !archived.success() {
            println!("! Copying the inputs failed; the target fetches them itself");
        }
        Ok(())
    }

    /// Stream the unit's journal until it ends; fail unless it succeeded.
    fn follow(&self, rebooting: bool) -> Result<()> {
        let host = &self.host;
        let script = format!(
            "journalctl -fu {UNIT} -o cat -n all & j=$!; \
             while [ \"$(systemctl show -p ActiveState --value {UNIT})\" != inactive ] && [ \"$(systemctl show -p ActiveState --value {UNIT})\" != failed ]; do sleep 2; done; \
             sleep 1; kill $j; echo {RESULT_MARK}$(systemctl show -p Result --value {UNIT})"
        );
        let mut child = self
            .ssh(true, &script)
            .stdout(Stdio::piped())
            .spawn()
            .context("follow the restore")?;
        let mut result = None;
        let mut saw_reboot = false;
        for line in BufReader::new(child.stdout.take().context("ssh stdout")?).lines() {
            let line = line.unwrap_or_default();
            if let Some(r) = line.strip_prefix(RESULT_MARK) {
                result = Some(r.to_string());
            } else {
                saw_reboot |= line.starts_with("Rebooting");
                println!("{line}");
            }
        }
        let _ = child.wait();
        match result.as_deref() {
            Some("success") => {
                println!("✓ Restore finished on {host}");
                Ok(())
            }
            None if rebooting && saw_reboot => {
                println!("✓ Restore finished; {host} is rebooting into the restored system");
                Ok(())
            }
            Some(r) => {
                bail!("the restore on {host} ended with {r}: ssh {host} journalctl -u {UNIT}")
            }
            None => bail!("lost the connection to {host}; the restore may still run: ssh {host} journalctl -fu {UNIT}"),
        }
    }
}

/// The neo revision the config's flake.lock pins (github inputs), else the default.
pub(crate) fn pinned_neo(repo: &Path) -> String {
    let lock = fs::read_to_string(repo.join("flake.lock")).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&lock).unwrap_or_default();
    let l = &v["nodes"]["neo"]["locked"];
    match (
        l["type"].as_str(),
        l["owner"].as_str(),
        l["repo"].as_str(),
        l["rev"].as_str(),
    ) {
        (Some("github"), Some(o), Some(r), Some(rev)) => format!("github:{o}/{r}/{rev}"),
        _ => "github:madebydamo/neo".into(),
    }
}

pub(crate) fn default_key() -> Option<PathBuf> {
    let k = PathBuf::from(env::var("HOME").ok()?).join(".ssh/id_ed25519");
    k.exists().then_some(k)
}

pub fn run(args: &RestoreArgs, host: &str) -> Result<()> {
    let key = args
        .key
        .clone()
        .or_else(default_key)
        .context("pass --key with an admin ssh-ed25519 private key")?;
    // Unreadable keys must show now, not after the kexec stopped the running system.
    for k in std::iter::once(&key).chain(&args.backup_key) {
        fs::File::open(k).with_context(|| format!("open {}", k.display()))?;
    }
    let target = Target::new(host, key.clone());
    let host = target.host.as_str();

    // Stage it on the client: it has the credentials for a private repo. Without
    // a repo, take it out of the backup here (the admin key opens it).
    let tmp = TempDir::new("neo-restore")?;
    let local = tmp.join("config");
    let from_backup = match &args.from_backup {
        Some(spec) => {
            if spec.starts_with('/') || spec.starts_with('.') {
                bail!("with --host, --from-backup takes user@host:folder, ssh://… or https://…; for a disk attached to the new machine use --data-from or run neo restore on it");
            }
            Some((spec, Source::from_location(spec)?))
        }
        None => None,
    };
    let repo = args.repo.as_deref();
    if let (None, Some((_, source))) = (repo, &from_backup) {
        let access = Access {
            identities: args
                .backup_key
                .iter()
                .cloned()
                .chain([key.clone()])
                .collect(),
            known_hosts: tmp.join("known_hosts"),
            work: tmp.join("backup"),
        };
        let source = backup::detect(&access, source.clone())?;
        println!(
            "Taking the config repo out of the backup ({}) …",
            source.describe()
        );
        backup::extract_config(&access, &source, &local)?;
        let _ = fs::remove_dir_all(tmp.join("backup"));
        commit_uncommitted(&local)?;
    } else {
        let repo = repo.context("give the config repo, or --from-backup <backup>")?;
        stage_repo(repo, &local)?;
    }
    let origin = match (&args.origin, repo) {
        (Some(o), _) => o.clone(),
        (None, None) => remote_url(&local),
        (None, Some(r)) => default_origin(r),
    };

    target.ensure_installer(args.yes)?;
    target.copy_repo_and_key(tmp.path())?;
    if let Some(k) = &args.backup_key {
        target
            .copy_private_file(k, BACKUP_KEY)
            .with_context(|| format!("copying --backup-key to {host} failed"))?;
    }
    let neo = target.neo(args, &local)?;
    target.copy_inputs(&local)?;
    // The clone holds the config: gone before the long part starts.
    drop(tmp);

    let cmd = format!(
        "{neo} {}",
        args.forwarded_argv(&origin)
            .iter()
            .map(|a| shell_quote(a))
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Plan first (no changes), then the run after a yes here.
    let check = format!("{cmd} --check");
    println!("Checking on {host} …");
    if !target
        .ssh(
            true,
            &format!("PATH=/run/current-system/sw/bin:$PATH {check}"),
        )
        .status()?
        .success()
    {
        target.remove_keys();
        bail!("the preflight on {host} failed (see above); nothing was changed");
    }
    if args.check {
        target.remove_keys();
        return Ok(());
    }
    if !args.yes {
        confirm("Restore with this plan?")?;
    }
    let reboot = if args.reboot { " --reboot" } else { "" };
    let start = format!(
        "systemctl reset-failed {UNIT} 2>/dev/null; systemd-run --unit={UNIT} -p LimitNOFILE=1048576 -p WorkingDirectory=/root -E HOME=/root -E PATH=/run/current-system/sw/bin {cmd} --yes --shred-key{reboot}"
    );
    execute_command(&mut target.ssh(true, &start))?;
    println!("Restore runs on {host} as unit {UNIT}; it keeps going if this connection drops (ssh root@… journalctl -fu {UNIT}).");
    target.follow(args.reboot)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The target's clap parses what the client sends.
    fn parse(argv: &[String]) -> RestoreArgs {
        use clap::{Args, FromArgMatches};
        let cmd = RestoreArgs::augment_args(clap::Command::new("neo"));
        let m = cmd
            .try_get_matches_from(
                std::iter::once("neo").chain(argv[1..].iter().map(String::as_str)),
            )
            .unwrap();
        RestoreArgs::from_arg_matches(&m).unwrap()
    }

    fn all_set() -> RestoreArgs {
        RestoreArgs {
            repo: Some("/home/me/config".into()),
            from_backup: Some("me@backup:it's here".into()),
            host: Some("root@box".into()),
            disk: Some("/dev/disk/by-id/a b".into()),
            key: Some("/home/me/.ssh/admin".into()),
            data: DataSource::None,
            data_from: Some("/mnt/old disk".into()),
            backup_key: Some("/home/me/.ssh/backup".into()),
            keep_hardware: true,
            check: true,
            yes: true,
            reboot: true,
            target: "/mnt2".into(),
            config: "other".into(),
            no_install: true,
            neo: Some("github:me/neo".into()),
            origin: Some("ignored".into()),
            shred_key: true,
        }
    }

    #[test]
    fn every_flag_reaches_the_target() {
        let args = all_set();
        let got = parse(&args.forwarded_argv("git@github.com:me/c.git"));
        assert_eq!(got.repo.as_deref(), Some("/root/neo-restore/repo"));
        assert_eq!(got.origin.as_deref(), Some("git@github.com:me/c.git"));
        assert_eq!(got.key, Some("/root/neo-restore/admin-key".into()));
        assert_eq!(got.backup_key, Some("/root/neo-restore/backup-key".into()));
        assert_eq!(got.from_backup, args.from_backup);
        assert_eq!(got.disk, args.disk);
        assert_eq!(got.data, args.data);
        assert_eq!(got.data_from, args.data_from);
        assert_eq!(got.target, args.target);
        assert_eq!(got.config, args.config);
        assert!(got.keep_hardware && got.no_install);
        // Client-only or added per stage.
        assert!(got.host.is_none() && got.neo.is_none());
        assert!(!got.check && !got.yes && !got.reboot && !got.shred_key);
    }

    #[test]
    fn defaults_stay_off_the_command_line() {
        let args = parse(&["neo".to_string(), "/repo".to_string()]);
        let argv = args.forwarded_argv("");
        assert!(!argv.iter().any(|a| a == "--target"), "{argv:?}");
        assert!(!argv.iter().any(|a| a == "--backup-key"), "{argv:?}");
        assert!(!argv.iter().any(|a| a == "--data"), "{argv:?}");
        // An empty origin still arrives, as "no remote".
        assert_eq!(parse(&argv).origin.as_deref(), Some(""));
    }

    /// Names every flag so a new one needs a decision here too (the exhaustive
    /// pattern in forwarded_argv covers the code, this the clap declaration).
    #[test]
    fn flags_are_forwarded_or_client_only() {
        use clap::Args;
        const FORWARDED: &[&str] = &[
            "from_backup",
            "disk",
            "data",
            "data_from",
            "backup_key",
            "keep_hardware",
            "target",
            "config",
            "no_install",
        ];
        const REPLACED: &[&str] = &["repo", "key", "origin"];
        const BY_STAGE: &[&str] = &["check", "yes", "reboot", "shred_key"];
        const CLIENT_ONLY: &[&str] = &["host", "neo"];
        let cmd = RestoreArgs::augment_args(clap::Command::new("neo"));
        for arg in cmd.get_arguments() {
            let id = arg.get_id().as_str();
            assert!(
                [FORWARDED, REPLACED, BY_STAGE, CLIENT_ONLY]
                    .iter()
                    .any(|l| l.contains(&id)),
                "--{id}: forward it in forwarded_argv or list it as client-only"
            );
        }
    }

    #[test]
    fn the_shell_on_the_target_keeps_arguments_whole() {
        let argv = all_set().forwarded_argv("it's");
        let line = argv
            .iter()
            .map(|a| shell_quote(a))
            .collect::<Vec<_>>()
            .join(" ");
        let out = Command::new("sh")
            .args(["-c", &format!("printf '%s\\n' {line}")])
            .output()
            .unwrap();
        let got: Vec<&str> = std::str::from_utf8(&out.stdout).unwrap().lines().collect();
        assert_eq!(got, argv);
    }

    #[test]
    fn ssh_options_carry_port_and_key() {
        let t = Target::new("root@box:2222", PathBuf::from("/k"));
        let o = t.ssh_opts(true).join(" ");
        assert!(o.contains("-p 2222") && o.ends_with("-i /k"), "{o}");
        assert!(o.contains("UserKnownHostsFile=/dev/null"));
        let o = Target::new("root@box", PathBuf::from("/k"))
            .ssh_opts(false)
            .join(" ");
        assert!(!o.contains("-p ") && o.contains("accept-new"), "{o}");
        assert_eq!(t.host, "root@box");
    }

    #[test]
    fn host_with_port() {
        assert_eq!(
            split_port("root@1.2.3.4:2222"),
            ("root@1.2.3.4".into(), Some("2222".into()))
        );
        assert_eq!(split_port("root@box"), ("root@box".into(), None));
        assert_eq!(
            split_port("root@[::1]:22"),
            ("root@::1".into(), Some("22".into()))
        );
        assert_eq!(split_port("root@::1"), ("root@::1".into(), None));
    }

    #[test]
    fn pinned_neo_from_lock() {
        let dir = env::temp_dir().join(format!("neo-pin-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("flake.lock"),
            r#"{"nodes":{"neo":{"locked":{"type":"github","owner":"madebydamo","repo":"neo","rev":"abc123"}}}}"#,
        )
        .unwrap();
        assert_eq!(pinned_neo(&dir), "github:madebydamo/neo/abc123");
        fs::write(
            dir.join("flake.lock"),
            r#"{"nodes":{"neo":{"locked":{"type":"git","url":"file:///x"}}}}"#,
        )
        .unwrap();
        assert_eq!(pinned_neo(&dir), "github:madebydamo/neo");
        let _ = fs::remove_dir_all(&dir);
    }
}
