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

use super::backup::{self, Access, Location, Source};
use super::local::{commit_uncommitted, default_origin, remote_url, stage_repo, WORK_DIR};
use super::{DataSource, RestoreArgs};
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
        self.copy_private_file(&self.key, "admin-key")
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
    let target = Target::new(host, key.clone());
    let host = target.host.as_str();

    // Stage it on the client: it has the credentials for a private repo. Without
    // a repo, take it out of the backup here (the admin key opens it).
    let tmp = TempDir::new("neo-restore")?;
    let local = tmp.join("config");
    let from_backup = match &args.from_backup {
        Some(spec) => {
            let source = Source::from_location(spec)?;
            if source.location == Location::Disk {
                bail!("with --host, --from-backup takes user@host:folder or ssh://…; for a disk attached to the new machine run neo restore on it");
            }
            Some((spec, source))
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
    let neo = target.neo(args, &local)?;
    target.copy_inputs(&local)?;
    // The clone holds the config: gone before the long part starts.
    drop(tmp);

    let mut cmd = vec![
        neo.clone(),
        "restore".into(),
        format!("{WORK_DIR}/repo"),
        "--origin".into(),
        shell_quote(&origin),
        "--key".into(),
        format!("{WORK_DIR}/admin-key"),
        "--config".into(),
        shell_quote(&args.config),
    ];
    if let Some(d) = &args.disk {
        cmd.extend(["--disk".into(), shell_quote(d)]);
    }
    if args.data == DataSource::None {
        cmd.extend(["--data".into(), "none".into()]);
    }
    if let Some(p) = &args.data_from {
        cmd.extend(["--data-from".into(), shell_quote(&p.display().to_string())]);
    }
    if let Some((spec, _)) = &from_backup {
        cmd.extend(["--from-backup".into(), shell_quote(spec)]);
    }
    if args.keep_hardware {
        cmd.push("--keep-hardware".into());
    }
    if args.no_install {
        cmd.push("--no-install".into());
    }

    // Plan first (no changes), then the run after a yes here.
    let check = format!("{} --check", cmd.join(" "));
    println!("Checking on {host} …");
    if !target
        .ssh(
            true,
            &format!("PATH=/run/current-system/sw/bin:$PATH {check}"),
        )
        .status()?
        .success()
    {
        bail!("the preflight on {host} failed (see above); nothing was changed");
    }
    if args.check {
        return Ok(());
    }
    if !args.yes {
        confirm("Restore with this plan?")?;
    }
    cmd.extend(["--yes".into(), "--shred-key".into()]);
    if args.reboot {
        cmd.push("--reboot".into());
    }
    let start = format!(
        "systemctl reset-failed {UNIT} 2>/dev/null; systemd-run --unit={UNIT} -p LimitNOFILE=1048576 -p WorkingDirectory=/root -E HOME=/root -E PATH=/run/current-system/sw/bin {}",
        cmd.join(" ")
    );
    execute_command(&mut target.ssh(true, &start))?;
    println!("Restore runs on {host} as unit {UNIT}; it keeps going if this connection drops (ssh root@… journalctl -fu {UNIT}).");
    target.follow(args.reboot)
}

#[cfg(test)]
mod tests {
    use super::*;

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
