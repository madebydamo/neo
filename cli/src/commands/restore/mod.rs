//! `neo restore <repo>`: rebuild a machine from its config repository and its
//! backup (docs/design/backup-restore.md §4, docs/RESTORE.md).
//!
//! - local (no `--host`): as root on a NixOS installer. Clone, unlock the
//!   escrowed host key, adapt the repo to this machine (disk, hardware config),
//!   Disko, data from the backup, `nixos-install`.
//! - remote (`--host root@IP`): from a client. nixos-anywhere kexecs the target
//!   into an installer when needed, then the local mode runs there.
//!
//! Without Disko in the config, the target must be partitioned and mounted by
//! hand (the old `neo restore --target /mnt`); restore installs into it.

use anyhow::{bail, Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::DocumentMut;

use crate::utils::execute_command;
use crate::utils::fsutil::write_private;
use crate::utils::secrets::{self, Node, Tree};
use crate::utils::secrets_store::{decrypt_with_key, IDENTITY_FILE};
use crate::utils::settings::{item_at, recorded_host_key};

pub mod backup;
pub(crate) mod local;
pub(crate) mod remote;

const DEFAULT_CONFIG_PATH: &str = "/var/neo/DATA/AppData/configuration";

/// What the data of the restored machine comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum DataSource {
    #[default]
    Backup,
    None,
}

#[derive(clap::Args, Debug, Clone)]
pub struct RestoreArgs {
    /// Config repository (git URL or local checkout; HEAD is restored).
    /// Leave it out with --from-backup to use the copy in the backup.
    #[arg(required_unless_present = "from_backup")]
    pub repo: Option<String>,
    /// Restore from this backup instead of the one in the settings:
    /// user@host:folder (relative to the home folder),
    /// ssh://user@host:port/folder, or a local folder (a mounted disk).
    /// An encrypted backup opens with --key.
    #[arg(long, value_name = "BACKUP")]
    pub from_backup: Option<String>,
    /// Restore that machine from here (root@IP[:PORT]). nixos-anywhere boots it
    /// into an installer first when it runs another system.
    #[arg(long)]
    pub host: Option<String>,
    /// Disk to install onto (sets disko.mainDisk), e.g. /dev/disk/by-id/….
    #[arg(long)]
    pub disk: Option<String>,
    /// Admin ssh-ed25519 private key (default: ~/.ssh/id_ed25519).
    #[arg(long)]
    pub key: Option<PathBuf>,
    /// `backup`: data from the backup configured in the repo; `none`: empty.
    #[arg(long, value_enum, default_value_t)]
    pub data: DataSource,
    /// Read a plain-files backup from this folder (e.g. a mounted disk).
    #[arg(long)]
    pub data_from: Option<PathBuf>,
    /// Another key that may log in to the backup.
    #[arg(long)]
    pub backup_key: Option<PathBuf>,
    /// Keep the repo's hardware-configuration.nix instead of generating one.
    #[arg(long)]
    pub keep_hardware: bool,
    /// Only check and print the plan.
    #[arg(long)]
    pub check: bool,
    /// Do not ask before erasing the disk.
    #[arg(long)]
    pub yes: bool,
    /// Reboot into the restored system when done.
    #[arg(long)]
    pub reboot: bool,
    /// Root of the target system.
    #[arg(long, default_value = "/mnt")]
    pub target: PathBuf,
    /// nixosConfigurations entry to install.
    #[arg(long, default_value = "neo")]
    pub config: String,
    /// Only clone the repo and restore the host key.
    #[arg(long)]
    pub no_install: bool,
    /// Neo flake ref to run on the target (remote mode).
    #[arg(long)]
    pub neo: Option<String>,
    /// Remote of the restored repo ("" for none; default: the repo URL).
    #[arg(long, hide = true)]
    pub origin: Option<String>,
    /// Delete --key when done.
    #[arg(long, hide = true)]
    pub shred_key: bool,
}

pub fn restore(args: &RestoreArgs, dry_run: bool) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: would restore {} {}",
            args.repo
                .as_deref()
                .or(args.from_backup.as_deref())
                .unwrap_or("-"),
            args.host
                .as_deref()
                .map(|h| format!("on {h}"))
                .unwrap_or_else(|| "on this machine".into())
        );
        return Ok(());
    }
    match &args.host {
        Some(host) => remote::run(args, host),
        None => local::run(args),
    }
}

pub(crate) fn leaf<'a>(tree: &'a Tree, path: &[&str]) -> Option<&'a Node> {
    let (first, rest) = path.split_first()?;
    let node = tree.get(*first)?;
    if rest.is_empty() {
        return Some(node);
    }
    match node {
        Node::Map(m) => leaf(m, rest),
        Node::Leaf(_) => None,
    }
}

/// Mount a ZFS dataset below the target, whatever its own mountpoint says.
pub(crate) fn mount_zfs(dataset: &str, path: &Path) -> Result<()> {
    execute_command(
        Command::new("mount")
            .args(["-t", "zfs", "-o", "zfsutil", dataset])
            .arg(path),
    )
}

/// Inside `target` (absolute path joined below it).
pub(crate) fn under(target: &Path, abs: &str) -> PathBuf {
    target.join(abs.trim_start_matches('/'))
}

pub(crate) fn int_setting(doc: &DocumentMut, key: &str, default: i64) -> i64 {
    item_at(doc, &["core", key])
        .and_then(|v| v.as_integer())
        .unwrap_or(default)
}

/// `access-tokens = host=token …` from whichever place holds core.git.tokens.
pub(crate) fn access_tokens(
    repo: &Path,
    doc: &DocumentMut,
    key: Option<&Path>,
) -> Result<Option<String>> {
    let mut tokens: Vec<(String, String)> = Vec::new();
    if let Some(t) = item_at(doc, &["core", "git", "tokens"]).and_then(|t| t.as_table_like()) {
        for (k, v) in t.iter() {
            if let Some(s) = v.as_str() {
                tokens.push((k.to_string(), s.to_string()));
            }
        }
    }
    let tree = if repo.join(secrets::SOPS_FILE).exists() {
        Some(decrypt_with_key(&repo.join(secrets::SOPS_FILE), key)?)
    } else if repo.join(secrets::PLAIN_FILE).exists() {
        Some(secrets::read_plain(&repo.join(secrets::PLAIN_FILE))?)
    } else {
        None
    };
    if let Some(Node::Map(m)) = tree
        .as_ref()
        .and_then(|t| leaf(t, &["core", "git", "tokens"]))
    {
        for (k, n) in m {
            if let Node::Leaf(v) = n {
                tokens.retain(|(h, _)| h != k);
                tokens.push((k.clone(), v.clone()));
            }
        }
    }
    if tokens.is_empty() {
        return Ok(None);
    }
    let line: Vec<String> = tokens.iter().map(|(h, t)| format!("{h}={t}")).collect();
    Ok(Some(format!("access-tokens = {}", line.join(" "))))
}

/// Write the escrowed host key into <target>/etc/ssh.
pub(crate) fn restore_identity(
    repo: &Path,
    target: &Path,
    doc: &DocumentMut,
    key: Option<&Path>,
) -> Result<bool> {
    let escrow = repo.join(IDENTITY_FILE);
    if !escrow.exists() {
        if repo.join(secrets::SOPS_FILE).exists() {
            bail!(
                "{} is encrypted but {IDENTITY_FILE} is missing: a new host key could not decrypt it. \
                 Decrypt and re-encrypt from a checkout first (`neo secrets disable` / `enable`).",
                secrets::SOPS_FILE
            );
        }
        return Ok(false);
    }
    let tree = decrypt_with_key(&escrow, key)?;
    let (Some(Node::Leaf(private)), Some(Node::Leaf(public))) = (
        leaf(&tree, &["ssh_host_ed25519_key"]),
        leaf(&tree, &["ssh_host_ed25519_key_pub"]),
    ) else {
        bail!("{IDENTITY_FILE} does not hold ssh_host_ed25519_key(_pub)");
    };
    let recorded = recorded_host_key(doc);
    let short = |k: &str| k.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
    if let Some(r) = recorded {
        if short(r) != short(public) {
            bail!("escrowed host key does not match core.secrets.hostKey; refusing to restore a foreign identity");
        }
    }
    let ssh = under(target, "/etc/ssh");
    fs::create_dir_all(&ssh).with_context(|| format!("create {}", ssh.display()))?;
    let priv_path = ssh.join("ssh_host_ed25519_key");
    let pub_path = ssh.join("ssh_host_ed25519_key.pub");
    write_private(&priv_path, private.as_bytes())?;
    fs::write(&pub_path, format!("{}\n", public.trim()))?;
    fs::set_permissions(&pub_path, fs::Permissions::from_mode(0o644))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_from_settings_and_plain_file() {
        let dir = std::env::temp_dir().join(format!("neo-restore-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(secrets::PLAIN_FILE),
            "core:\n  git:\n    tokens:\n      github.com: \"pat\"\n",
        )
        .unwrap();
        let doc: DocumentMut =
            "[core.git.tokens]\n\"gitlab.com\" = \"gl\"\n\"github.com\" = \"old\"\n"
                .parse()
                .unwrap();
        let line = access_tokens(&dir, &doc, None).unwrap().unwrap();
        assert!(line.contains("gitlab.com=gl"));
        assert!(line.contains("github.com=pat") && !line.contains("old"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn under_joins_absolute_paths() {
        assert_eq!(
            under(Path::new("/mnt"), "/etc/ssh"),
            PathBuf::from("/mnt/etc/ssh")
        );
    }
}
