//! Secret storage of a config repo (docs/design/secrets.md): inline in
//! settings.toml → secrets.yaml (plain) → secrets.sops.yaml (encrypted).
//! Shared by `neo secrets …` and the web UI Secrets card.
//!
//! Encryption only needs public keys: the recipients are the host key plus
//! every ssh-ed25519 key in core.ssh.authorizedKeys. Decryption needs the host
//! key, so on the machine it goes through the root helper `neo-secrets-root`
//! (sudo); elsewhere (a laptop checkout) sops uses the caller's own SSH key.

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use toml_edit::{value, DocumentMut, Item, Table};

use super::secrets::{self, Backend, Node, Tree};
use super::{git_cmd, sort_document_alphabetically};

pub const IDENTITY_FILE: &str = "identity.sops.yaml";
pub const SOPS_CONFIG: &str = ".sops.yaml";
const HOST_PUB: &str = "/etc/ssh/ssh_host_ed25519_key.pub";
const ROOT_HELPER: &str = "/run/current-system/sw/bin/neo-secrets-root";

fn env_or(var: &str, default: &str) -> String {
    std::env::var(var)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn sops_bin() -> String {
    env_or("SOPS_BINARY_PATH", "sops")
}

fn root_helper() -> Option<String> {
    let p = env_or("NEO_SECRETS_ROOT_CMD", ROOT_HELPER);
    Path::new(&p).is_file().then_some(p)
}

fn is_root() -> bool {
    // SAFETY-free: read our uid from /proc instead of libc.
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(1).map(|u| u == "0"))
        })
        .unwrap_or(false)
}

/// Run the root helper (directly as root, else `sudo -n`).
fn helper_output(helper: &str, args: &[&str]) -> Result<Vec<u8>> {
    let out = if is_root() {
        Command::new(helper).args(args).output()
    } else {
        Command::new(env_or("SUDO_BINARY_PATH", "sudo"))
            .arg("-n")
            .arg(helper)
            .args(args)
            .output()
    }
    .with_context(|| format!("run {helper}"))?;
    if !out.status.success() {
        bail!(
            "{helper} {} failed: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

/// `ssh-ed25519 <base64>` without the comment (what age expects).
fn normalize_ed25519(line: &str) -> Option<String> {
    let mut parts = line.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some("ssh-ed25519"), Some(b64)) => Some(format!("ssh-ed25519 {b64}")),
        _ => None,
    }
}

/// Admin recipients: every ssh-ed25519 key in core.ssh.authorizedKeys.
pub fn admin_keys(doc: &DocumentMut) -> Vec<String> {
    let mut keys: Vec<String> = doc
        .get("core")
        .and_then(|c| c.get("ssh"))
        .and_then(|s| s.get("authorizedKeys"))
        .and_then(|k| k.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .filter_map(normalize_ed25519)
                .collect()
        })
        .unwrap_or_default();
    keys.sort();
    keys.dedup();
    keys
}

/// Authorized keys that can log in but not decrypt (not ssh-ed25519).
pub fn ignored_keys(doc: &DocumentMut) -> Vec<String> {
    doc.get("core")
        .and_then(|c| c.get("ssh"))
        .and_then(|s| s.get("authorizedKeys"))
        .and_then(|k| k.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .filter(|k| normalize_ed25519(k).is_none())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// The machine's public host key: from this machine, else the one recorded in settings.
pub fn host_key(doc: &DocumentMut) -> Option<String> {
    fs::read_to_string(env_or("NEO_HOST_PUBLIC_KEY", HOST_PUB))
        .ok()
        .and_then(|s| normalize_ed25519(s.trim()))
        .or_else(|| {
            doc.get("core")
                .and_then(|c| c.get("secrets"))
                .and_then(|s| s.get("hostKey"))
                .and_then(|k| k.as_str())
                .and_then(normalize_ed25519)
        })
}

fn record_host_key(doc: &mut DocumentMut, host: &str) {
    if !doc.get("core").is_some_and(|c| c.is_table()) {
        doc.insert("core", Item::Table(Table::new()));
    }
    let core = doc["core"].as_table_mut().expect("core table");
    if !core.get("secrets").is_some_and(|s| s.is_table()) {
        core.insert("secrets", Item::Table(Table::new()));
    }
    core["secrets"]["hostKey"] = value(host);
}

/// Encrypt a tree to the given ssh-ed25519 recipients (no private key needed).
pub fn encrypt(tree: &Tree, recipients: &[String]) -> Result<String> {
    if recipients.is_empty() {
        bail!("no recipients to encrypt to");
    }
    let mut child = Command::new(sops_bin())
        .args([
            "encrypt",
            "--age",
            &recipients.join(","),
            "--input-type",
            "yaml",
            "--output-type",
            "yaml",
            "/dev/stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("run sops encrypt (is sops installed?)")?;
    child
        .stdin
        .take()
        .context("sops stdin")?
        .write_all(secrets::emit(tree).as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!(
            "sops encrypt failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    String::from_utf8(out.stdout).context("sops output is not UTF-8")
}

fn json_to_tree(v: &serde_json::Value) -> Result<Tree> {
    let obj = v.as_object().context("decrypted secrets are not a map")?;
    let mut tree = Tree::new();
    for (k, v) in obj {
        let node = match v {
            serde_json::Value::Object(_) => Node::Map(json_to_tree(v)?),
            serde_json::Value::String(s) => Node::Leaf(s.clone()),
            serde_json::Value::Null => continue,
            other => Node::Leaf(other.to_string()),
        };
        tree.insert(k.clone(), node);
    }
    Ok(tree)
}

/// Decrypt a .sops.yaml file: the root helper (host key) on the machine,
/// else sops with the caller's own SSH key (~/.ssh/id_ed25519).
pub fn decrypt(path: &Path) -> Result<Tree> {
    let abs = fs::canonicalize(path).with_context(|| format!("resolve {}", path.display()))?;
    match root_helper() {
        Some(helper) => {
            let json = helper_output(&helper, &["decrypt", &abs.to_string_lossy()])?;
            let v: serde_json::Value =
                serde_json::from_slice(&json).context("parse decrypted secrets")?;
            json_to_tree(&v)
        }
        None => decrypt_with_key(&abs, None),
    }
}

/// Decrypt with sops directly, using `key` (an ssh-ed25519 private key) or
/// sops' default (~/.ssh/id_ed25519, prompting for its passphrase).
pub fn decrypt_with_key(path: &Path, key: Option<&Path>) -> Result<Tree> {
    let mut cmd = Command::new(sops_bin());
    cmd.args(["decrypt", "--input-type", "yaml", "--output-type", "json"])
        .arg(path)
        .stdin(Stdio::inherit())
        .stderr(Stdio::piped());
    if let Some(k) = key {
        cmd.env("SOPS_AGE_SSH_PRIVATE_KEY_FILE", k);
    }
    let out = cmd
        .output()
        .context("run sops decrypt (is sops installed?)")?;
    if !out.status.success() {
        bail!(
            "sops decrypt of {} failed (is your ssh-ed25519 key a recipient?): {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).context("parse decrypted secrets")?;
    json_to_tree(&v)
}

/// identity.sops.yaml: the host key encrypted to the admins only (machine only).
fn escrow(admins: &[String]) -> Result<Option<String>> {
    let Some(helper) = root_helper() else {
        return Ok(None);
    };
    let mut args = vec!["escrow"];
    args.extend(admins.iter().map(String::as_str));
    let out = helper_output(&helper, &args)?;
    Ok(Some(
        String::from_utf8(out).context("escrow output is not UTF-8")?,
    ))
}

/// .sops.yaml so a plain `sops edit` on a checkout uses the same recipients.
pub fn sops_config(host: &str, admins: &[String]) -> String {
    let mut all = vec![host.to_string()];
    all.extend(admins.iter().cloned());
    format!(
        "# Generated by neo from core.ssh.authorizedKeys (ssh-ed25519) and core.secrets.hostKey.\n\
         # Saving SSH keys in the web UI (or `neo secrets rekey`) re-encrypts and rewrites it.\n\
         creation_rules:\n\
         \x20 - path_regex: (^|/)identity\\.sops\\.yaml$\n\
         \x20   age: \"{}\"\n\
         \x20 - path_regex: (^|/)secrets\\.sops\\.yaml$\n\
         \x20   age: \"{}\"\n",
        admins.join(","),
        all.join(",")
    )
}

/// Files of one config repo.
pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.toml")
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn backend(&self) -> Backend {
        secrets::backend_for(&self.settings_path())
    }

    fn load_doc(&self) -> Result<DocumentMut> {
        let p = self.settings_path();
        let s = fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
        s.parse().context("parse settings.toml")
    }

    fn save_doc(&self, doc: &mut DocumentMut) -> Result<()> {
        sort_document_alphabetically(doc);
        fs::write(self.settings_path(), doc.to_string()).context("write settings.toml")
    }

    /// Stage these files: existing ones added, missing ones removed from the index.
    fn git_add(&self, names: &[&str]) -> Result<()> {
        let dir = self.dir.to_string_lossy().to_string();
        let (present, gone): (Vec<&str>, Vec<&str>) =
            names.iter().partition(|n| self.path(n).exists());
        if !present.is_empty() {
            let mut args = vec!["add", "--"];
            args.extend_from_slice(&present);
            git_cmd(&dir, &args)?;
        }
        if !gone.is_empty() {
            let mut args = vec!["rm", "-q", "--cached", "--ignore-unmatch", "--"];
            args.extend_from_slice(&gone);
            git_cmd(&dir, &args)?;
        }
        Ok(())
    }

    fn write_atomic(&self, name: &str, content: &str) -> Result<()> {
        let tmp = self.path(&format!(".{name}.tmp"));
        fs::write(&tmp, content).with_context(|| format!("write {}", tmp.display()))?;
        fs::rename(&tmp, self.path(name)).with_context(|| format!("replace {name}"))
    }

    /// Whether the config's Nix files read the secrets files (the deploy
    /// template's settings.nix since secrets support). An older settings.nix
    /// (modules not refreshed from the template by `neo update` since) would
    /// evaluate without the moved values.
    pub fn loads_secret_files(&self) -> bool {
        fn scan(dir: &Path, depth: usize) -> bool {
            let Ok(entries) = fs::read_dir(dir) else {
                return false;
            };
            entries.flatten().any(|e| {
                let path = e.path();
                let name = e.file_name();
                let name = name.to_string_lossy();
                if path.is_dir() {
                    depth > 0
                        && !name.starts_with('.')
                        && name != "result"
                        && scan(&path, depth - 1)
                } else {
                    name.ends_with(".nix")
                        && fs::read_to_string(&path).is_ok_and(|t| t.contains(secrets::SOPS_FILE))
                }
            })
        }
        scan(&self.dir, 4)
    }

    /// Move inline secret settings into secrets.yaml. Returns the moved paths.
    pub fn split(&self, patterns: &[Vec<String>]) -> Result<Vec<Vec<String>>> {
        if !self.loads_secret_files() {
            bail!(
                "this configuration does not load {} / {} yet (modules/settings.nix is older than secrets support): \
                 refresh it from the neo homeserver template first",
                secrets::PLAIN_FILE,
                secrets::SOPS_FILE
            );
        }
        let mut tree = match self.backend() {
            Backend::Sops(_) => bail!("secrets are already encrypted"),
            Backend::Plain(p) => secrets::read_plain(&p)?,
            Backend::Inline => Tree::new(),
        };
        let mut doc = self.load_doc()?;
        let split = secrets::split_into(&mut doc, &mut tree, patterns, None);
        let plain = self.path(secrets::PLAIN_FILE);
        if split.moved.is_empty() && plain.exists() {
            return Ok(vec![]);
        }
        // Secrets first: if settings.toml fails, the values exist twice and the
        // secrets file wins on evaluation.
        secrets::write_plain(&plain, &tree)?;
        self.save_doc(&mut doc)?;
        self.git_add(&[secrets::PLAIN_FILE, "settings.toml"])?;
        Ok(split.moved)
    }

    /// Encrypt the secrets for the host and the admins. Verifies the result
    /// decrypts to the same values before the plain file is removed.
    pub fn enable(&self, patterns: &[Vec<String>]) -> Result<EnableReport> {
        if matches!(self.backend(), Backend::Sops(_)) {
            bail!("secrets are already encrypted");
        }
        let doc = self.load_doc()?;
        let admins = admin_keys(&doc);
        if admins.is_empty() {
            bail!("core.ssh.authorizedKeys has no ssh-ed25519 key: nobody could decrypt or restore the secrets");
        }
        let host = host_key(&doc)
            .context("no host key: run this on the machine (/etc/ssh/ssh_host_ed25519_key.pub)")?;
        self.split(patterns)?;
        let tree = secrets::read_plain(&self.path(secrets::PLAIN_FILE))?;
        let mut recipients = vec![host.clone()];
        recipients.extend(admins.iter().cloned());
        let encrypted = encrypt(&tree, &recipients)?;

        // Prove the machine (or this admin) can read it back before dropping the plain file.
        let check = self.path(".verify.sops.yaml");
        fs::write(&check, &encrypted)?;
        let back = decrypt(&check);
        let _ = fs::remove_file(&check);
        if back.context("decrypting the freshly encrypted secrets")? != prune_tree(&tree) {
            bail!("decrypted secrets differ from secrets.yaml; nothing changed");
        }

        let escrowed = match escrow(&admins)? {
            Some(text) => {
                self.write_atomic(IDENTITY_FILE, &text)?;
                true
            }
            None => false,
        };
        self.write_atomic(SOPS_CONFIG, &sops_config(&host, &admins))?;
        // Reload: split() rewrote settings.toml.
        let mut fresh = self.load_doc()?;
        record_host_key(&mut fresh, &host);
        self.save_doc(&mut fresh)?;
        self.write_atomic(secrets::SOPS_FILE, &encrypted)?;
        fs::remove_file(self.path(secrets::PLAIN_FILE)).context("remove secrets.yaml")?;
        self.git_add(&[
            secrets::PLAIN_FILE,
            secrets::SOPS_FILE,
            IDENTITY_FILE,
            SOPS_CONFIG,
            "settings.toml",
        ])?;
        Ok(EnableReport {
            values: count_leaves(&tree),
            recipients: recipients.len(),
            escrowed,
        })
    }

    /// Decrypt back to a plain secrets.yaml.
    pub fn disable(&self) -> Result<usize> {
        let Backend::Sops(sops) = self.backend() else {
            bail!("secrets are not encrypted");
        };
        let tree = decrypt(&sops)?;
        secrets::write_plain(&self.path(secrets::PLAIN_FILE), &tree)?;
        for name in [secrets::SOPS_FILE, IDENTITY_FILE, SOPS_CONFIG] {
            let _ = fs::remove_file(self.path(name));
        }
        self.git_add(&[
            secrets::PLAIN_FILE,
            secrets::SOPS_FILE,
            IDENTITY_FILE,
            SOPS_CONFIG,
        ])?;
        Ok(count_leaves(&tree))
    }

    /// Recipients this repo should be encrypted to now.
    fn recipients(&self, doc: &DocumentMut) -> Result<(String, Vec<String>)> {
        let admins = admin_keys(doc);
        if admins.is_empty() {
            bail!("core.ssh.authorizedKeys has no ssh-ed25519 key left: refusing to re-encrypt for the host alone");
        }
        let host = host_key(doc).context("no host key recorded (core.secrets.hostKey)")?;
        Ok((host, admins))
    }

    /// Whether core.ssh.authorizedKeys changed since the last encryption.
    pub fn needs_rekey(&self) -> bool {
        if !matches!(self.backend(), Backend::Sops(_)) {
            return false;
        }
        let Ok(doc) = self.load_doc() else {
            return false;
        };
        let Ok((host, admins)) = self.recipients(&doc) else {
            return false;
        };
        fs::read_to_string(self.path(SOPS_CONFIG)).ok() != Some(sops_config(&host, &admins))
    }

    /// Re-encrypt for the current host + admin keys and re-escrow the host key.
    pub fn rekey(&self) -> Result<usize> {
        let Backend::Sops(sops) = self.backend() else {
            bail!("secrets are not encrypted");
        };
        let doc = self.load_doc()?;
        let (host, admins) = self.recipients(&doc)?;
        let tree = decrypt(&sops)?;
        let mut recipients = vec![host.clone()];
        recipients.extend(admins.iter().cloned());
        let encrypted = encrypt(&tree, &recipients)?;
        if let Some(text) = escrow(&admins)? {
            self.write_atomic(IDENTITY_FILE, &text)?;
        }
        self.write_atomic(SOPS_CONFIG, &sops_config(&host, &admins))?;
        self.write_atomic(secrets::SOPS_FILE, &encrypted)?;
        self.git_add(&[secrets::SOPS_FILE, IDENTITY_FILE, SOPS_CONFIG])?;
        Ok(recipients.len())
    }

    /// Apply a settings save to the encrypted file: changed secrets are set,
    /// cleared ones (within `scope`) removed, placeholders left alone. Writes
    /// only when something changed, so unrelated saves never re-encrypt.
    pub fn save_encrypted(
        &self,
        doc: &mut DocumentMut,
        patterns: &[Vec<String>],
        scope: Option<&[String]>,
    ) -> Result<bool> {
        let Backend::Sops(sops) = self.backend() else {
            bail!("secrets are not encrypted");
        };
        // Dry run on the key structure (readable without decrypting): no new
        // value and nothing cleared → no sudo, no re-encryption.
        let keys_before = secrets::read_plain(&sops)?;
        let mut keys = keys_before.clone();
        let mut probe = doc.clone();
        let split = secrets::split_into(&mut probe, &mut keys, patterns, scope);
        if split.moved.is_empty() && prune_tree(&keys) == prune_tree(&keys_before) {
            *doc = probe;
            return Ok(false);
        }
        let mut tree = decrypt(&sops)?;
        let before = tree.clone();
        secrets::split_into(doc, &mut tree, patterns, scope);
        if prune_tree(&tree) == prune_tree(&before) {
            return Ok(false);
        }
        let current = self.load_doc()?;
        let (host, admins) = self.recipients(&current)?;
        let mut recipients = vec![host];
        recipients.extend(admins);
        let encrypted = encrypt(&prune_tree(&tree), &recipients)?;
        self.write_atomic(secrets::SOPS_FILE, &encrypted)?;
        Ok(true)
    }
}

pub struct EnableReport {
    pub values: usize,
    pub recipients: usize,
    pub escrowed: bool,
}

fn count_leaves(tree: &Tree) -> usize {
    tree.values()
        .map(|n| match n {
            Node::Leaf(_) => 1,
            Node::Map(m) => count_leaves(m),
        })
        .sum()
}

/// Tree without empty maps (what emit/sops round-trips to).
fn prune_tree(tree: &Tree) -> Tree {
    tree.iter()
        .filter_map(|(k, n)| match n {
            Node::Map(m) => {
                let m = prune_tree(m);
                (!m.is_empty()).then(|| (k.clone(), Node::Map(m)))
            }
            leaf => Some((k.clone(), leaf.clone())),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_refuses_a_settings_nix_without_secrets_support() {
        let dir = std::env::temp_dir().join(format!("neo-store-guard-{}", std::process::id()));
        fs::create_dir_all(dir.join("modules")).unwrap();
        fs::write(dir.join("settings.toml"), "[services.a]\ntoken = \"x\"\n").unwrap();
        fs::write(dir.join("modules/settings.nix"), "{...}: {}\n").unwrap();
        let store = Store::new(&dir);
        let patterns = vec![vec![
            "services".to_string(),
            "a".to_string(),
            "token".to_string(),
        ]];
        assert!(store.split(&patterns).is_err());
        assert!(fs::read_to_string(dir.join("settings.toml"))
            .unwrap()
            .contains("token"));
        fs::write(
            dir.join("modules/settings.nix"),
            "sopsFile = ../secrets.sops.yaml;\n",
        )
        .unwrap();
        assert!(store.loads_secret_files());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn admin_keys_keep_only_ed25519_without_comment() {
        let doc: DocumentMut = r#"
[core.ssh]
authorizedKeys = ["ssh-ed25519 AAAA1 me@x", "ssh-rsa BBBB you@y", "ssh-ed25519 AAAA1 dup", "sk-ssh-ed25519@openssh.com CCCC k"]
"#
        .parse()
        .unwrap();
        assert_eq!(admin_keys(&doc), vec!["ssh-ed25519 AAAA1".to_string()]);
        assert_eq!(ignored_keys(&doc).len(), 2);
    }

    #[test]
    fn sops_config_lists_admins_for_identity_and_host_for_secrets() {
        let c = sops_config("ssh-ed25519 HOST", &["ssh-ed25519 A".into()]);
        assert!(c.contains("identity\\.sops\\.yaml$\n    age: \"ssh-ed25519 A\""));
        assert!(c.contains("secrets\\.sops\\.yaml$\n    age: \"ssh-ed25519 HOST,ssh-ed25519 A\""));
    }

    #[test]
    fn json_to_tree_stringifies_scalars() {
        let t = json_to_tree(&serde_json::json!({"a": {"b": "x", "n": 3}, "z": null})).unwrap();
        let Node::Map(a) = &t["a"] else { panic!() };
        assert_eq!(a["n"], Node::Leaf("3".into()));
        assert!(!t.contains_key("z"));
    }
}
