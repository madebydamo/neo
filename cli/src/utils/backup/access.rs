//! How this machine reaches a backup destination: ssh, sftp and commands
//! run where the backup is.

use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::source::{Location, Source};
use crate::utils::command::{output_with_stdin, shell_join, shell_words, stdout_checked};

/// The admin key `neo restore` and `neo backup` use when none is given:
/// `~/.ssh/id_ed25519`, when it exists.
pub(crate) fn default_key() -> Option<PathBuf> {
    let k = PathBuf::from(env::var("HOME").ok()?).join(".ssh/id_ed25519");
    k.exists().then_some(k)
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

    pub(super) fn ssh_line(&self, source: &Source) -> String {
        shell_join("ssh", self.ssh_args(source))
    }

    /// An executable ssh with these keys and options, for `sftp -S` and
    /// restic's sftp.command (both pass ssh's own arguments).
    pub(super) fn ssh_wrapper(&self, source: &Source) -> Result<PathBuf> {
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
    pub(super) fn sftp(&self, source: &Source, batch: &str) -> Result<std::process::Output> {
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
    pub(super) fn fetch(&self, source: &Source, path: &str, out: &Path) -> Result<()> {
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

    pub(super) fn command(&self, source: &Source, argv: &[&str]) -> Command {
        match &source.location {
            Location::Ssh { dest, .. } => {
                let mut c = Command::new("ssh");
                c.args(self.ssh_args(source))
                    .arg(dest)
                    .arg(shell_words(argv));
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
