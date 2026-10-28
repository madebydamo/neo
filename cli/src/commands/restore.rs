//! `neo restore <repo>`: reinstall a machine from its config repository
//! (docs/design/secrets.md §6). Run as root on a NixOS live system with the
//! target filesystems mounted (disko / hardware config are out of scope).
//!
//!   1. clone the repo into <target>/<neo-cli.server.configPath>
//!   2. decrypt identity.sops.yaml with an admin SSH key → <target>/etc/ssh host key,
//!      so the reinstalled machine decrypts its secrets and keeps its identity
//!   3. decrypt core.git.tokens for private flake inputs (NIX_CONFIG access-tokens)
//!   4. hand the repo to the neo user, then `nixos-install --flake <repo>#<config>`

use anyhow::{bail, Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::DocumentMut;

use crate::utils::execute_command;
use crate::utils::secrets::{self, Node, Tree};
use crate::utils::secrets_store::{decrypt_with_key, IDENTITY_FILE};

const DEFAULT_CONFIG_PATH: &str = "/var/neo/DATA/AppData/configuration";

pub struct RestoreArgs {
    pub repo: String,
    pub target: PathBuf,
    pub key: Option<PathBuf>,
    pub config: String,
    pub no_install: bool,
}

fn leaf<'a>(tree: &'a Tree, path: &[&str]) -> Option<&'a Node> {
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
fn under(target: &Path, abs: &str) -> PathBuf {
    target.join(abs.trim_start_matches('/'))
}

fn int_setting(doc: &DocumentMut, key: &str, default: i64) -> i64 {
    doc.get("core")
        .and_then(|c| c.get(key))
        .and_then(|v| v.as_integer())
        .unwrap_or(default)
}

/// `access-tokens = host=token …` from whichever place holds core.git.tokens.
fn access_tokens(repo: &Path, doc: &DocumentMut, key: Option<&Path>) -> Result<Option<String>> {
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
fn restore_identity(
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

pub fn restore(args: &RestoreArgs, dry_run: bool) -> Result<()> {
    if !args.target.is_dir() {
        bail!(
            "target {} does not exist (mount the new system there first)",
            args.target.display()
        );
    }
    let staging = std::env::temp_dir().join(format!("neo-restore-{}", std::process::id()));
    println!("Cloning {} …", args.repo);
    execute_command(
        Command::new("git")
            .args(["clone", &args.repo])
            .arg(&staging),
    )?;
    let doc: DocumentMut = fs::read_to_string(staging.join("settings.toml"))
        .context("the repository has no settings.toml")?
        .parse()
        .context("parse settings.toml")?;
    let config_path = doc
        .get("neo-cli")
        .and_then(|n| n.get("server"))
        .and_then(|s| s.get("configPath"))
        .and_then(|p| p.as_str())
        .unwrap_or(DEFAULT_CONFIG_PATH)
        .to_string();
    let repo = under(&args.target, &config_path);
    if repo.exists() && fs::read_dir(&repo)?.next().is_some() {
        bail!("{} already exists and is not empty", repo.display());
    }
    if dry_run {
        println!(
            "DRY-RUN: would restore into {} and install #{}",
            repo.display(),
            args.config
        );
        let _ = fs::remove_dir_all(&staging);
        return Ok(());
    }
    if let Some(parent) = repo.parent() {
        fs::create_dir_all(parent)?;
    }
    execute_command(
        Command::new("git")
            .args(["clone", "--no-hardlinks"])
            .arg(&staging)
            .arg(&repo),
    )?;
    execute_command(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["remote", "set-url", "origin", &args.repo]),
    )?;
    let _ = fs::remove_dir_all(&staging);

    let key = args.key.as_deref();
    let restored = restore_identity(&repo, &args.target, &doc, key)?;
    if restored {
        println!("✓ Host key restored from {IDENTITY_FILE}: same identity, secrets decrypt on first boot");
    } else {
        println!("! No {IDENTITY_FILE}: the machine gets a new host key (re-trust it on clients)");
    }
    let tokens = access_tokens(&repo, &doc, key)?;

    let uid = int_setting(&doc, "uid", 1000);
    let gid = int_setting(&doc, "gid", 1000);
    execute_command(
        Command::new("chown")
            .arg("-R")
            .arg(format!("{uid}:{gid}"))
            .arg(&repo),
    )?;

    if args.no_install {
        println!(
            "Config repository at {} (skipped nixos-install)",
            repo.display()
        );
        return Ok(());
    }
    let mut install = Command::new("nixos-install");
    install
        .arg("--root")
        .arg(&args.target)
        .arg("--no-root-passwd")
        .arg("--flake")
        .arg(format!("{}#{}", repo.display(), args.config))
        .env("NIX_CONFIG", {
            let mut c = "experimental-features = nix-command flakes\n".to_string();
            if let Some(t) = &tokens {
                c.push_str(t);
                c.push('\n');
            }
            c
        });
    execute_command(&mut install)?;
    println!("Installed. Reboot into the restored system; appdata comes back from your backups.");
    Ok(())
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
