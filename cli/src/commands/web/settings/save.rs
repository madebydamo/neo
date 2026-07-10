use std::fs;
use std::path::Path;
use std::sync::Arc;

use rocket::http::Status;
use toml_edit::{DocumentMut, Item, Table};

use super::{insert_dotted, json_to_toml_item, json_to_toml_value};
use crate::commands::web::action_bar::broadcast_action_bar;
use crate::commands::web::structs::AppConfig;

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

pub fn write_settings_doc(path: &Path, doc: &DocumentMut) -> Result<(), Status> {
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

/// After a successful settings write: refresh nix evaluator and push action-bar state.
pub fn after_save(config: &AppConfig) {
    let ev = config.evaluator.clone();
    tokio::spawn(async move {
        let mut g = ev.lock().await;
        let _ = g.refresh().await;
    });
    broadcast_action_bar(config);
}

pub fn finish_save(path: &Path, doc: &DocumentMut, config: &AppConfig) -> Status {
    if write_settings_doc(path, doc).is_err() {
        return Status::InternalServerError;
    }
    after_save(config);
    Status::Ok
}

/// Convenience when the caller holds `State<Arc<AppConfig>>`.
pub fn finish_save_state(path: &Path, doc: &DocumentMut, config: &Arc<AppConfig>) -> Status {
    finish_save(path, doc, config.as_ref())
}
