//! `neo settings get|patch`: read and change settings.toml from a script, the
//! same edits the web UI makes (`web::settings::edit::apply_ops`), with secret
//! values routed into the secrets file when the config keeps one. `neo
//! bootstrap` uses `patch` to add a tunnel to the streamproxy edge.

use anyhow::{bail, Context, Result};
use std::io::Read;
use std::path::Path;
use toml_edit::DocumentMut;

use super::secrets::secret_paths;
use super::web::settings::edit::{apply_ops, PatchOp};
use super::web::settings::toml_item_to_json;
use crate::utils::secrets_store::Store;
use crate::utils::secrets_yaml::{self, Backend};
use crate::utils::settings::{self, item_at};

#[derive(clap::Subcommand, Debug)]
pub enum SettingsAction {
    /// Print one setting of configPath/settings.toml as JSON (null when unset).
    /// Values kept in a secrets file are not shown.
    Get {
        /// Dotted path, e.g. services.swag.domain
        path: String,
    },
    /// Apply changes read as JSON from stdin:
    /// [{"set": {"path": ["services","x","enabled"], "value": true}},
    ///  {"unset": {"path": [...], "secret": false}}].
    /// Secret values go to the secrets file when the config keeps one.
    Patch,
}

pub fn settings(
    action: SettingsAction,
    config_path: &str,
    nix_cmd: &str,
    dry_run: bool,
) -> Result<()> {
    let path = Path::new(config_path).join(settings::FILE);
    match action {
        SettingsAction::Get { path: key } => {
            let doc = settings::load(&path)?;
            let parts: Vec<&str> = key.split('.').collect();
            let value = item_at(&doc, &parts)
                .and_then(toml_item_to_json)
                .unwrap_or(serde_json::Value::Null);
            println!("{}", serde_json::to_string(&value)?);
            Ok(())
        }
        SettingsAction::Patch => {
            let mut input = String::new();
            std::io::stdin()
                .read_to_string(&mut input)
                .context("read the changes from stdin")?;
            let ops: Vec<PatchOp> = serde_json::from_str(&input)
                .context("parse the changes (JSON list of set/unset)")?;
            if dry_run {
                for op in &ops {
                    match op {
                        PatchOp::Set { path, .. } => println!("DRY-RUN: set {}", path.join(".")),
                        PatchOp::Unset { path, .. } => {
                            println!("DRY-RUN: unset {}", path.join("."))
                        }
                    }
                }
                return Ok(());
            }
            patch(config_path, &ops, nix_cmd)?;
            println!("Saved {} change(s) to {}", ops.len(), path.display());
            Ok(())
        }
    }
}

/// Apply `ops` to configPath/settings.toml like a web UI save.
pub fn patch(config_path: &str, ops: &[PatchOp], nix_cmd: &str) -> Result<()> {
    let path = Path::new(config_path).join(settings::FILE);
    let mut doc: DocumentMut = if path.exists() {
        settings::load(&path)?
    } else {
        DocumentMut::new()
    };
    apply_ops(&mut doc, ops).map_err(anyhow::Error::msg)?;
    let store = Store::new(config_path);
    match secrets_yaml::backend_for(&path) {
        Backend::Inline => {}
        backend => {
            if !store.loads_secret_files() {
                bail!(
                    "modules/settings.nix does not load the secrets files: refresh the config first (neo update)"
                );
            }
            // Set values, then the cleared ones (scope = that path), as the web UI does.
            let patterns = secret_paths(config_path, nix_cmd)?;
            let cleared: Vec<&Vec<String>> = ops
                .iter()
                .filter_map(|op| match op {
                    PatchOp::Unset { path, secret: true } => Some(path),
                    _ => None,
                })
                .collect();
            let scopes =
                std::iter::once(None).chain(cleared.into_iter().map(|p| Some(p.as_slice())));
            for scope in scopes {
                match &backend {
                    Backend::Plain(file) => {
                        let mut tree = secrets_yaml::read_plain(file)?;
                        secrets_yaml::split_into(&mut doc, &mut tree, &patterns, scope);
                        secrets_yaml::write_plain(file, &tree)?;
                    }
                    Backend::Sops(_) => {
                        store
                            .save_encrypted(&mut doc, &patterns, scope)
                            .context("encrypted secrets")?;
                    }
                    Backend::Inline => {}
                }
            }
        }
    }
    settings::save(&path, &mut doc)?;
    if store.needs_rekey() {
        store
            .rekey()
            .context("saved, but re-encrypting the secrets for the new SSH keys failed")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::fsutil::TempDir;
    use std::fs;

    #[test]
    fn patch_writes_inline_settings_sorted() {
        let dir = TempDir::new("neo-settings-cli-test").unwrap();
        fs::write(
            dir.join("settings.toml"),
            "[services.swag]\nenabled = true\n",
        )
        .unwrap();
        let ops: Vec<PatchOp> = serde_json::from_str(
            r#"[{"set": {"path": ["services","streamproxy","entries","home"], "value": {"url": "home.example.com", "token": "t", "wildcard": true}}},
                {"set": {"path": ["services","streamproxy","enabled"], "value": true}}]"#,
        )
        .unwrap();
        patch(dir.path().to_str().unwrap(), &ops, "nix").unwrap();
        let doc = settings::load(&dir.join("settings.toml")).unwrap();
        assert_eq!(
            doc["services"]["streamproxy"]["entries"]["home"]["url"].as_str(),
            Some("home.example.com")
        );
        assert_eq!(
            doc["services"]["streamproxy"]["enabled"].as_bool(),
            Some(true)
        );
        assert_eq!(doc["services"]["swag"]["enabled"].as_bool(), Some(true));
    }
}
