//! What one evaluation of the config repo read (docs/design/eval-latency.md §4).
//!
//! The settings files (`settings.toml` and the secrets files) are values: a
//! change there keeps the module structure, so the web UI can serve the last
//! evaluation with the new values laid over it and re-evaluate in the
//! background. Every other `.nix` / `.toml` / `.lock` file, plus
//! `core.plugins` (read at flake level by `_plugins.nix`), is structure.
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::commands::web::settings::toml_item_to_json;
use crate::utils::secrets::{self, Node, Tree, PLACEHOLDER_PREFIX};

pub const SETTINGS_FILE: &str = "settings.toml";

/// Files read as data by `modules/settings.nix` (copied for an in-process rebind).
pub const SETTINGS_FILES: [&str; 3] = [SETTINGS_FILE, secrets::PLAIN_FILE, secrets::SOPS_FILE];

/// How two snapshots differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    /// Only settings values: the last evaluation stays a good approximation.
    Values,
    /// Modules, inputs or plugins: pages that list services wait for the new one.
    Structural,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Path, size and mtime of every file (cheap probe).
    pub stat: u64,
    /// Contents of every file.
    pub content: u64,
    /// Contents of the non-settings files plus `core.plugins`.
    pub structure: u64,
    /// The `neo` tree as settings.nix builds it: settings.toml with the secrets
    /// laid over it (encrypted values are their placeholders, like in Nix).
    /// Null while settings.toml does not parse.
    pub settings: Value,
    /// Settings file name → bytes, as read (absent files are left out).
    pub(super) files: Vec<(&'static str, Vec<u8>)>,
}

impl Snapshot {
    pub fn read(dir: &Path) -> Self {
        let paths = config_files(dir);
        let mut content = DefaultHasher::new();
        let mut structure = DefaultHasher::new();
        let mut files = Vec::new();
        for p in &paths {
            // Unreadable hashes as empty; a later readable state then differs.
            let bytes = fs::read(p).unwrap_or_default();
            p.hash(&mut content);
            bytes.hash(&mut content);
            match settings_file_name(dir, p) {
                Some(name) => files.push((name, bytes)),
                None => {
                    p.hash(&mut structure);
                    bytes.hash(&mut structure);
                }
            }
        }
        let settings = merged_settings(&files);
        settings
            .get("core")
            .and_then(|c| c.get("plugins"))
            .map(Value::to_string)
            .hash(&mut structure);
        Self {
            stat: stat_hash(&paths),
            content: content.finish(),
            structure: structure.finish(),
            settings,
            files,
        }
    }

    /// `None` when both read the same files.
    pub fn change_to(&self, newer: &Snapshot) -> Option<ChangeKind> {
        if self.structure != newer.structure {
            Some(ChangeKind::Structural)
        } else if self.content != newer.content {
            Some(ChangeKind::Values)
        } else {
            None
        }
    }

    /// Copy the settings files into `to` (a directory settings.nix can read).
    pub fn write_files(&self, to: &Path) -> std::io::Result<()> {
        fs::create_dir_all(to)?;
        for (name, bytes) in &self.files {
            fs::write(to.join(name), bytes)?;
        }
        Ok(())
    }
}

/// [`Snapshot::stat`] without reading contents (run every second by the pool).
pub fn stat_probe(dir: &Path) -> u64 {
    stat_hash(&config_files(dir))
}

fn settings_file_name(dir: &Path, p: &Path) -> Option<&'static str> {
    if p.parent() != Some(dir) {
        return None;
    }
    let name = p.file_name()?.to_str()?;
    SETTINGS_FILES.into_iter().find(|f| *f == name)
}

/// `settings.toml`, then the secrets on top (`lib.neo.secrets.load`).
fn merged_settings(files: &[(&'static str, Vec<u8>)]) -> Value {
    let file = |name: &str| {
        files
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
    };
    let mut tree = match file(SETTINGS_FILE).map(|t| t.parse::<toml_edit::DocumentMut>()) {
        // Unparseable: unknown, not empty (the overlay must not reset every value).
        Some(Err(_)) => return Value::Null,
        Some(Ok(doc)) => toml_item_to_json(doc.as_item()),
        None => None,
    }
    .unwrap_or_else(|| Value::Object(Default::default()));
    if let Some(Ok(plain)) = file(secrets::PLAIN_FILE).map(|t| secrets::parse(&t)) {
        merge(&mut tree, tree_to_json(&plain, &mut Vec::new(), false));
    }
    if let Some(Ok(sops)) = file(secrets::SOPS_FILE).map(|t| secrets::parse(&t)) {
        merge(&mut tree, tree_to_json(&sops, &mut Vec::new(), true));
    }
    tree
}

fn tree_to_json(tree: &Tree, path: &mut Vec<String>, placeholders: bool) -> Value {
    let mut out = serde_json::Map::new();
    for (k, node) in tree {
        path.push(k.clone());
        let v = match node {
            Node::Map(m) => tree_to_json(m, path, placeholders),
            Node::Leaf(_) if placeholders => {
                Value::String(format!("{PLACEHOLDER_PREFIX}{}@", path.join("/")))
            }
            Node::Leaf(s) => Value::String(s.clone()),
        };
        path.pop();
        out.insert(k.clone(), v);
    }
    Value::Object(out)
}

/// `lib.recursiveUpdate`: objects merge, anything else replaces.
fn merge(into: &mut Value, from: Value) {
    match (into, from) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                match a.get_mut(&k) {
                    Some(slot) if slot.is_object() && v.is_object() => merge(slot, v),
                    _ => {
                        a.insert(k, v);
                    }
                }
            }
        }
        (slot, v) => *slot = v,
    }
}

/// The files the evaluated flake sees: `.nix` / `.toml` / `.lock` anywhere and
/// the secrets files at the top (skipping dot entries, `result` and `target`).
fn config_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "result" || name == "target" {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            walk(root, &path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "nix" || e == "toml" || e == "lock")
            || settings_file_name(root, &path).is_some()
        {
            out.push(path);
        }
    }
}

fn stat_hash(files: &[PathBuf]) -> u64 {
    let mut h = DefaultHasher::new();
    for p in files {
        p.hash(&mut h);
        if let Ok(meta) = fs::metadata(p) {
            meta.len().hash(&mut h);
            meta.modified().ok().hash(&mut h);
        }
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config() -> PathBuf {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neo-snap-{}-{n}", std::process::id()));
        fs::create_dir_all(dir.join("modules")).unwrap();
        fs::write(dir.join("settings.toml"), "[services.a]\nenabled = true\n").unwrap();
        fs::write(dir.join("modules/x.nix"), "{}\n").unwrap();
        dir
    }

    fn bump_mtime(p: &Path) {
        let f = fs::File::options().write(true).open(p).unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        f.set_modified(later).unwrap();
    }

    #[test]
    fn touch_and_unrelated_files_change_nothing() {
        let dir = temp_config();
        let before = Snapshot::read(&dir);
        bump_mtime(&dir.join("settings.toml"));
        fs::write(dir.join("notes.txt"), "x").unwrap();
        fs::write(dir.join("modules/other.yaml"), "x").unwrap();
        fs::create_dir_all(dir.join("result")).unwrap();
        fs::write(dir.join("result/x.nix"), "x").unwrap();
        let after = Snapshot::read(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert_ne!(before.stat, after.stat);
        assert_eq!(before.change_to(&after), None);
    }

    #[test]
    fn settings_and_secrets_are_values() {
        let dir = temp_config();
        let base = Snapshot::read(&dir);
        fs::write(dir.join("settings.toml"), "[services.a]\nenabled = false\n").unwrap();
        let edited = Snapshot::read(&dir);
        fs::write(
            dir.join("secrets.yaml"),
            "services:\n  a:\n    token: \"t\"\n",
        )
        .unwrap();
        let secret = Snapshot::read(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(base.change_to(&edited), Some(ChangeKind::Values));
        assert_eq!(edited.change_to(&secret), Some(ChangeKind::Values));
        assert_eq!(edited.settings["services"]["a"]["enabled"], false);
        assert_eq!(secret.settings["services"]["a"]["token"], "t");
        assert_eq!(secret.settings["services"]["a"]["enabled"], false);
    }

    #[test]
    fn unparseable_settings_are_unknown() {
        let dir = temp_config();
        fs::write(dir.join("settings.toml"), "[services.a\nenabled = \n").unwrap();
        let broken = Snapshot::read(&dir);
        fs::remove_file(dir.join("settings.toml")).unwrap();
        let missing = Snapshot::read(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert!(broken.settings.is_null());
        assert_eq!(missing.settings, serde_json::json!({}));
    }

    #[test]
    fn modules_and_plugins_are_structure() {
        let dir = temp_config();
        let base = Snapshot::read(&dir);
        fs::write(dir.join("modules/x.nix"), "{ a = 1; }\n").unwrap();
        let module = Snapshot::read(&dir);
        fs::write(
            dir.join("settings.toml"),
            "[services.a]\nenabled = true\n[core.plugins.p]\nurl = \"github:x/y\"\n",
        )
        .unwrap();
        let plugin = Snapshot::read(&dir);
        fs::write(dir.join("flake.lock"), "{}").unwrap();
        let lock = Snapshot::read(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(base.change_to(&module), Some(ChangeKind::Structural));
        assert_eq!(module.change_to(&plugin), Some(ChangeKind::Structural));
        assert_eq!(plugin.change_to(&lock), Some(ChangeKind::Structural));
    }

    #[test]
    fn sops_values_are_placeholders_and_files_copy() {
        let dir = temp_config();
        fs::write(
            dir.join("secrets.sops.yaml"),
            "services:\n    a:\n        token: ENC[AES256_GCM,data:x]\nsops:\n    version: 3.9.0\n",
        )
        .unwrap();
        let snap = Snapshot::read(&dir);
        let out = dir.join("copy");
        snap.write_files(&out).unwrap();
        let copied = fs::read_to_string(out.join("secrets.sops.yaml")).unwrap();
        let toml = fs::read_to_string(out.join("settings.toml")).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            snap.settings["services"]["a"]["token"],
            "@NEO_SECRET:services/a/token@"
        );
        assert!(snap.settings.get("sops").is_none());
        assert!(copied.contains("ENC[AES256_GCM"));
        assert!(toml.contains("enabled = true"));
        assert!(!out.join("secrets.yaml").exists());
    }
}
