//! Remote mode: `neo restore <repo> --host root@IP` from a client. Brings the
//! target into a NixOS installer (nixos-anywhere kexec) when it is not one,
//! copies the repo, the admin key and this neo into its RAM and runs the local
//! mode there as a systemd unit, streaming its journal back.

use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use super::backup::{self, Access, Location, Source};
use super::local::{commit_uncommitted, remote_url, WORK_DIR};
use super::{DataSource, RestoreArgs};
use crate::utils::command::{shell_quote, stdout_checked};
use crate::utils::execute_command;
use crate::utils::fsutil::TempDir;

const UNIT: &str = "neo-restore";

/// SSH port of the target (`--host root@IP:PORT`), for every ssh/nix/nixos-anywhere call.
static PORT: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
/// The admin key also logs in to the target (besides the agent and ssh's defaults).
static KEY: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

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

/// ssh options for the target. `ephemeral`: the installer's host key changes
/// with every kexec, so it is never written to known_hosts.
fn ssh_opts(ephemeral: bool) -> Vec<String> {
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
    if let Some(Some(p)) = PORT.get() {
        o.extend(["-p".to_string(), p.clone()]);
    }
    if let Some(k) = KEY.get() {
        o.extend(["-i".to_string(), k.display().to_string()]);
    }
    o
}

fn ssh(host: &str, ephemeral: bool, script: &str) -> Command {
    let mut c = Command::new("ssh");
    c.args(ssh_opts(ephemeral)).arg(host).arg(script);
    c
}

fn ssh_ok(host: &str, ephemeral: bool, script: &str) -> bool {
    ssh(host, ephemeral, script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A NixOS installer: root on tmpfs (or the ISO's overlay) and nixos-install.
const INSTALLER_TEST: &str = "test -e /etc/NIXOS && command -v nixos-install >/dev/null && case $(findmnt -no FSTYPE /) in tmpfs|overlay|rootfs) true;; *) false;; esac";

fn confirm(question: &str) -> Result<()> {
    print!("{question} Type yes to continue: ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    if line.trim() != "yes" {
        bail!("aborted");
    }
    Ok(())
}

fn kexec(host: &str) -> Result<()> {
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
        host,
    ]);
    if let Some(Some(port)) = PORT.get() {
        anywhere.args(["--ssh-port", port]);
    }
    if let Some(k) = KEY.get() {
        // Not `-i`: that ssh-adds the key to the user's agent (a gpg-agent then
        // asks for a passphrase to store it).
        anywhere
            .arg("--ssh-option")
            .arg(format!("IdentityFile={}", k.display()));
    }
    execute_command(&mut anywhere)?;
    println!("Waiting for the installer …");
    let start = Instant::now();
    while !ssh_ok(host, true, INSTALLER_TEST) {
        if start.elapsed() > Duration::from_secs(600) {
            bail!("{host} did not come back as a NixOS installer");
        }
        sleep(Duration::from_secs(5));
    }
    Ok(())
}

/// Where the running neo comes from: the wrapper's store path, copied to the
/// target, or a flake ref run there.
fn neo_on_target(host: &str, args: &RestoreArgs, repo: &Path) -> Result<String> {
    if let Some(r) = &args.neo {
        return Ok(format!(
            "nix --extra-experimental-features 'nix-command flakes' run {}#neo --",
            shell_quote(r)
        ));
    }
    if let Ok(pkg) = env::var("NEO_PACKAGE") {
        if pkg.starts_with("/nix/store/") && Path::new(&pkg).exists() {
            println!("Copying this neo to {host} …");
            let copied = Command::new("nix")
                .args([
                    "--extra-experimental-features",
                    "nix-command",
                    "copy",
                    "--no-check-sigs",
                    "--to",
                ])
                .arg(format!("ssh-ng://{host}"))
                .arg(&pkg)
                .env("NIX_SSHOPTS", ssh_opts(true).join(" "))
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
    let (host, port) = split_port(host);
    let _ = PORT.set(port);
    let host = host.as_str();
    let key = args
        .key
        .clone()
        .or_else(default_key)
        .context("pass --key with an admin ssh-ed25519 private key")?;
    let _ = KEY.set(key.clone());

    // Clone on the client: it has the credentials for a private repo. Without
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
        execute_command(Command::new("git").args(["clone", "-q", repo]).arg(&local))?;
    }
    let origin = match (&args.origin, repo) {
        (Some(o), _) => o.clone(),
        (None, None) => remote_url(&local),
        (None, Some(r)) if Path::new(r).is_dir() => remote_url(Path::new(r)),
        (None, Some(r)) => r.to_string(),
    };

    // An installer already? Otherwise kexec into one (the disk is not touched).
    let installer = ssh_ok(host, true, INSTALLER_TEST);
    if !installer {
        if !ssh_ok(host, false, "true") {
            bail!("cannot log in to {host} as root with your SSH key");
        }
        println!(
            "{host} is not a NixOS installer: nixos-anywhere will boot one into its RAM (kexec)."
        );
        if !args.yes {
            confirm("The running system on it stops.")?;
        }
        kexec(host)?;
    }

    // Repo, admin key, neo.
    println!("Copying the config repo and your key into the installer's RAM …");
    let mut tar = Command::new("tar")
        .arg("-C")
        .arg(tmp.path())
        .args(["-cf", "-", "config"])
        .stdout(Stdio::piped())
        .spawn()
        .context("tar")?;
    let unpack = ssh(
        host,
        true,
        &format!("set -e; rm -rf {WORK_DIR}/repo {WORK_DIR}/config; mkdir -p {WORK_DIR}; chmod 700 {WORK_DIR}; tar -C {WORK_DIR} -xf -; mv {WORK_DIR}/config {WORK_DIR}/repo; chown -R root:root {WORK_DIR}/repo"),
    )
    .stdin(tar.stdout.take().context("tar stdout")?)
    .status()?;
    if !tar.wait()?.success() || !unpack.success() {
        bail!("copying the repo to {host} failed");
    }
    let key_file = fs::File::open(&key).with_context(|| format!("open {}", key.display()))?;
    if !ssh(
        host,
        true,
        &format!("umask 077; cat > {WORK_DIR}/admin-key"),
    )
    .stdin(key_file)
    .status()?
    .success()
    {
        bail!("copying the key to {host} failed");
    }
    let neo = neo_on_target(host, args, &local)?;
    // Every locked input (nixpkgs, neo, plugins) into the installer's store:
    // fetched here with this computer's credentials, so private plugins need
    // no tokens there and the target does not download them again.
    println!("Copying the config's inputs to {host} …");
    let archived = Command::new("nix")
        .args([
            "--extra-experimental-features",
            "nix-command flakes",
            "flake",
            "archive",
            "--no-check-sigs",
            "--to",
        ])
        .arg(format!("ssh-ng://{host}"))
        .arg(&local)
        .env("NIX_SSHOPTS", ssh_opts(true).join(" "))
        .status()
        .context("nix flake archive")?;
    if !archived.success() {
        println!("! Copying the inputs failed; the target fetches them itself");
    }
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
    if !ssh(
        host,
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
    execute_command(&mut ssh(host, true, &start))?;
    println!("Restore runs on {host} as unit {UNIT}; it keeps going if this connection drops (ssh root@… journalctl -fu {UNIT}).");
    follow(host, args.reboot)
}

/// Stream the unit's journal until it ends; fail unless it succeeded.
fn follow(host: &str, rebooting: bool) -> Result<()> {
    let script = format!(
        "journalctl -fu {UNIT} -o cat -n all & j=$!; \
         while [ \"$(systemctl show -p ActiveState --value {UNIT})\" != inactive ] && [ \"$(systemctl show -p ActiveState --value {UNIT})\" != failed ]; do sleep 2; done; \
         sleep 1; kill $j; echo {RESULT_MARK}$(systemctl show -p Result --value {UNIT})"
    );
    let mut child = ssh(host, true, &script)
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
        Some(r) => bail!("the restore on {host} ended with {r}: ssh {host} journalctl -u {UNIT}"),
        None => bail!("lost the connection to {host}; the restore may still run: ssh {host} journalctl -fu {UNIT}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
