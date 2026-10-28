use std::fs;
use std::path::Path;

use rocket::http::Status;
use toml_edit::{DocumentMut, Table};

use super::{insert_dotted, json_to_toml_item, json_to_toml_value};
use crate::commands::web::action_bar::broadcast_action_bar;
use crate::commands::web::types::AppConfig;
use crate::utils::secrets_store::Store;
use crate::utils::{secrets, sort_document_alphabetically};

pub fn load_settings_doc(path: &Path) -> Result<DocumentMut, Status> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = if path.exists() {
        fs::read_to_string(path).map_err(|_| Status::InternalServerError)?
    } else {
        String::new()
    };
    if content.trim().is_empty() {
        Ok(DocumentMut::new())
    } else {
        content.parse().map_err(|_| Status::InternalServerError)
    }
}

pub fn write_settings_doc(path: &Path, doc: &mut DocumentMut) -> Result<(), Status> {
    sort_document_alphabetically(doc);
    fs::write(path, doc.to_string()).map_err(|_| Status::InternalServerError)
}

/// Apply a JSON object payload into a TOML table (supports dotted leaf keys).
pub fn apply_payload_to_table(
    table: &mut Table,
    payload: &serde_json::Map<String, serde_json::Value>,
) {
    for (k, v) in payload.iter() {
        if k.contains('.') {
            if let Some(tval) = json_to_toml_value(v) {
                insert_dotted(table, k, tval);
            }
        } else if let Some(titem) = json_to_toml_item(v) {
            table.insert(k, titem);
        }
    }
}

/// Refresh evaluator, action bar, and schema cache after settings.toml changes
/// (save or restore). Safe to call from routes that rewrite settings without
/// going through [`finish_save`].
pub fn refresh_after_settings_change(config: &AppConfig) {
    let ev = config.evaluator.clone();
    tokio::spawn(async move {
        let mut g = ev.lock().await;
        let _ = g.refresh().await;
    });
    broadcast_action_bar(config);
    let cache = config.schema_cache.clone();
    tokio::spawn(async move {
        let mut c = cache.write().await;
        c.invalidate_all();
    });
}

/// Move secret settings out of `doc` once the config repo keeps them in a
/// secrets file (docs/design/secrets.md §3.3). Without one they stay inline, as
/// before. `scope` is the table this save replaced: a secret missing from it
/// was cleared in the form and leaves secrets.yaml too.
pub async fn route_secrets(
    config: &AppConfig,
    doc: &mut DocumentMut,
    scope: Option<&[String]>,
) -> Result<(), Status> {
    let backend = secrets::backend_for(&config.settings_path);
    if backend == secrets::Backend::Inline {
        return Ok(());
    }
    // A settings.nix older than secrets support never reads the secrets file:
    // a value saved there would silently drop out of the evaluated config.
    let store = Store::new(config.settings_path.parent().unwrap_or(Path::new(".")));
    if !store.loads_secret_files() {
        eprintln!(
            "web: refusing save; modules/settings.nix does not load {} / {} (refresh the config from the neo homeserver template, e.g. neo update)",
            secrets::PLAIN_FILE,
            secrets::SOPS_FILE
        );
        return Err(Status::InternalServerError);
    }
    // Without the list of secret paths a save could write secrets inline: refuse.
    let patterns = {
        let mut ev = config.evaluator.lock().await;
        ev.extract_secret_paths().await.map_err(|e| {
            eprintln!("web: refusing save; secret path extract failed: {e:#}");
            Status::InternalServerError
        })?
    };
    if patterns.is_empty() {
        eprintln!("web: refusing save; no secret settings found (evaluation problem?)");
        return Err(Status::InternalServerError);
    }
    match backend {
        secrets::Backend::Inline => Ok(()),
        secrets::Backend::Plain(path) => {
            let mut tree = secrets::read_plain(&path).map_err(|e| {
                eprintln!("web: refusing save: {e:#}");
                Status::InternalServerError
            })?;
            secrets::split_into(doc, &mut tree, &patterns, scope);
            secrets::write_plain(&path, &tree).map_err(|e| {
                eprintln!("web: writing secrets failed: {e:#}");
                Status::InternalServerError
            })
        }
        secrets::Backend::Sops(_) => {
            // Placeholders (unchanged values) are dropped; changed or cleared
            // secrets are re-encrypted (decrypting needs the root helper).
            store
                .save_encrypted(doc, &patterns, scope)
                .map(|_| ())
                .map_err(|e| {
                    eprintln!("web: refusing save; encrypted secrets: {e:#}");
                    Status::InternalServerError
                })
        }
    }
}

pub fn finish_save(path: &Path, doc: &mut DocumentMut, config: &AppConfig) -> Status {
    if write_settings_doc(path, doc).is_err() {
        return Status::InternalServerError;
    }
    // New or removed SSH keys change who may decrypt: re-encrypt right away.
    let store = Store::new(path.parent().unwrap_or(Path::new(".")));
    if store.needs_rekey() {
        if let Err(e) = store.rekey() {
            eprintln!("web: re-encrypting secrets for the new SSH keys failed: {e:#}");
            return Status::InternalServerError;
        }
    }
    refresh_after_settings_change(config);
    Status::Ok
}
