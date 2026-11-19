//! The encrypted format: a restic repository and neo-backup.age, its data key
//! encrypted with age to the host key and the admin keys (backup.sh).

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use super::access::Access;
use super::source::{join_remote, Location, Source};
use crate::utils::command::execute_command;
use crate::utils::command::{output_checked, stdout_checked};
use crate::utils::fsutil::write_private;

/// Encrypted format: the data key next to the restic repository, encrypted
/// with age to the host key and the admin keys (backup.sh).
pub const HEADER: &str = "neo-backup.age";

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
            .with_context(|| {
                format!(
                    "{} holds no backup yet (the first backup to this place has not finished)",
                    self.key.repository
                )
            })?;
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
    pub(super) fn restore(&self, target: &Path, exclude: &[&str], include: &[&str]) -> Result<()> {
        // The header goes up before the first upload, so the repository of an
        // interrupted first run may hold no snapshot yet.
        self.latest()?;
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
    pub(super) fn config_repos(&self) -> Result<Vec<String>> {
        self.latest()?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
