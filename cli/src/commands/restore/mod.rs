//! `neo restore <repo>`: rebuild a machine from its config repository and its
//! backup (docs/design/backup-restore.md §3, docs/RESTORE.md).
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
use toml_edit::DocumentMut;

use crate::utils::secrets::{self, Node, Tree};
use crate::utils::secrets_store::{decrypt_with_key, IDENTITY_FILE};

pub mod backup;
mod local;
mod remote;

const DEFAULT_CONFIG_PATH: &str = "/var/neo/DATA/AppData/configuration";

#[derive(Debug, Clone, Default)]
pub struct RestoreArgs {
    pub repo: String,
    pub target: PathBuf,
    pub key: Option<PathBuf>,
    pub config: String,
    pub no_install: bool,
    /// Run on this machine (root@IP) from a client.
    pub host: Option<String>,
    /// New disko.mainDisk.
    pub disk: Option<String>,
    /// Restore the data from the configured backup (false: --data none).
    pub data: bool,
    /// Read a files backup from this folder instead of the configured place.
    pub data_from: Option<PathBuf>,
    /// Extra key for the backup login.
    pub backup_key: Option<PathBuf>,
    pub keep_hardware: bool,
    pub yes: bool,
    pub reboot: bool,
    /// Preflight and plan only.
    pub check: bool,
    /// Remote of the restored repo (default: the repo URL).
    pub origin: Option<String>,
    /// Delete --key when done (remote mode copies it into the installer's RAM).
    pub shred_key: bool,
    /// Flake ref of the neo that runs on the target (remote mode).
    pub neo: Option<String>,
}

pub fn restore(args: &RestoreArgs, dry_run: bool) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: would restore {} {}",
            args.repo,
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

/// Inside `target` (absolute path joined below it).
pub(crate) fn under(target: &Path, abs: &str) -> PathBuf {
    target.join(abs.trim_start_matches('/'))
}

pub(crate) fn int_setting(doc: &DocumentMut, key: &str, default: i64) -> i64 {
    doc.get("core")
        .and_then(|c| c.get(key))
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
    if let Some(t) = doc
        .get("core")
        .and_then(|c| c.get("git"))
        .and_then(|g| g.get("tokens"))
        .and_then(|t| t.as_table_like())
    {
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
    let recorded = doc
        .get("core")
        .and_then(|c| c.get("secrets"))
        .and_then(|s| s.get("hostKey"))
        .and_then(|k| k.as_str());
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
    fs::write(&priv_path, private)?;
    fs::set_permissions(&priv_path, fs::Permissions::from_mode(0o600))?;
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
