use std::fs;
use std::io::Cursor;
use std::path::Path;

use rocket::http::{ContentType, Status};
use rocket::response::{Responder, Response};
use rocket::Request;
use toml_edit::{DocumentMut, Table};

use super::{insert_dotted, json_to_toml_item, json_to_toml_value};
use crate::commands::web::locks::Blocked;
use crate::commands::web::types::AppConfig;
use crate::commands::web::util::proc;
use crate::utils::{secrets, sort_document_alphabetically};

/// Why a settings write did not happen: 409 from the settings lock, else a status
/// and plain-text reason, which the form and the proposal card show as they are.
/// Never put a settings value in a reason (a TOML parse error quotes the line).
#[derive(Debug)]
pub enum SaveError {
    Blocked(Blocked),
    Refused(Status, String),
}

impl SaveError {
    pub fn internal(msg: impl Into<String>) -> Self {
        SaveError::Refused(Status::InternalServerError, msg.into())
    }

    pub fn message(&self) -> &str {
        match self {
            SaveError::Blocked(b) => &b.0,
            SaveError::Refused(_, msg) => msg,
        }
    }
}

impl From<Blocked> for SaveError {
    fn from(b: Blocked) -> Self {
        SaveError::Blocked(b)
    }
}

impl<'r> Responder<'r, 'static> for SaveError {
    fn respond_to(self, req: &'r Request<'_>) -> rocket::response::Result<'static> {
        match self {
            SaveError::Blocked(b) => b.respond_to(req),
            SaveError::Refused(status, msg) => {
                eprintln!("web: settings not saved: {msg}");
                Response::build()
                    .status(status)
                    .header(ContentType::Plain)
                    .sized_body(msg.len(), Cursor::new(msg))
                    .ok()
            }
        }
    }
}

pub fn load_settings_doc(path: &Path) -> Result<DocumentMut, SaveError> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = if path.exists() {
        fs::read_to_string(path).map_err(|e| SaveError::internal(format!("read settings: {e}")))?
    } else {
        String::new()
    };
    if content.trim().is_empty() {
        Ok(DocumentMut::new())
    } else {
        content
            .parse()
            .map_err(|_| SaveError::internal("settings.toml is not valid TOML"))
    }
}

pub fn write_settings_doc(path: &Path, doc: &mut DocumentMut) -> Result<(), SaveError> {
    sort_document_alphabetically(doc);
    fs::write(path, doc.to_string())
        .map_err(|e| SaveError::internal(format!("write settings: {e}")))
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

/// Evaluate changed settings in the background after settings.toml changes
/// (save or restore); pages keep being served meanwhile, with the new values laid
/// over (docs/design/eval-latency.md). The action-bar watcher notices the change
/// by itself. Safe to call from routes that rewrite settings without going
/// through [`finish_save`].
pub fn refresh_after_settings_change(config: &AppConfig) {
    config.eval.note_change("settings saved");
}

/// Move secret settings out of `doc` once the config repo keeps them in a
/// secrets file (docs/design/secrets.md §3.3). Without one they stay inline, as
/// before. `scope` is the table this save replaced: a secret missing from it
/// was cleared in the form and leaves secrets.yaml too.
pub async fn route_secrets(
    config: &AppConfig,
    doc: &mut DocumentMut,
    scope: Option<&[String]>,
) -> Result<(), SaveError> {
    let backend = secrets::backend_for(&config.settings_path);
    if backend == secrets::Backend::Inline {
        return Ok(());
    }
    // A settings.nix older than secrets support never reads the secrets file:
    // a value saved there would silently drop out of the evaluated config.
    let store = config.secrets_store();
    if !store.loads_secret_files() {
        return Err(SaveError::internal(format!(
            "Not saved: modules/settings.nix does not load {} / {} (refresh the config from the neo homeserver template, e.g. neo update)",
            secrets::PLAIN_FILE,
            secrets::SOPS_FILE
        )));
    }
    // Without the list of secret paths a save could write secrets inline: refuse.
    let patterns = config.eval.secret_paths().await.map_err(|e| {
        SaveError::internal(format!(
            "Not saved: could not list the secret settings: {e:#}"
        ))
    })?;
    if patterns.is_empty() {
        return Err(SaveError::internal(
            "Not saved: no secret settings found (evaluation problem?)",
        ));
    }
    match backend {
        secrets::Backend::Inline => Ok(()),
        secrets::Backend::Plain(path) => {
            let mut tree = secrets::read_plain(&path)
                .map_err(|e| SaveError::internal(format!("Not saved: {e:#}")))?;
            secrets::split_into(doc, &mut tree, &patterns, scope);
            secrets::write_plain(&path, &tree)
                .map_err(|e| SaveError::internal(format!("Writing secrets failed: {e:#}")))
        }
        secrets::Backend::Sops(_) => {
            // Placeholders (unchanged values) are dropped; changed or cleared
            // secrets are re-encrypted (decrypting needs the root helper).
            let scope = scope.map(<[String]>::to_vec);
            let mut owned = std::mem::take(doc);
            let (owned, saved) = proc::blocking(move || {
                let saved = store.save_encrypted(&mut owned, &patterns, scope.as_deref());
                (owned, saved)
            })
            .await
            .map_err(|e| SaveError::internal(format!("Not saved: encrypted secrets: {e}")))?;
            *doc = owned;
            saved
                .map(|_| ())
                .map_err(|e| SaveError::internal(format!("Not saved: encrypted secrets: {e:#}")))
        }
    }
}

pub async fn finish_save(
    path: &Path,
    doc: &mut DocumentMut,
    config: &AppConfig,
) -> Result<(), SaveError> {
    write_settings_doc(path, doc)?;
    // New or removed SSH keys change who may decrypt: re-encrypt right away.
    let store = config.secrets_store();
    proc::blocking(move || {
        if store.needs_rekey() {
            store.rekey().map(|_| ()).map_err(|e| format!("{e:#}"))
        } else {
            Ok(())
        }
    })
    .await
    .and_then(|r| r)
    .map_err(|e| {
        SaveError::internal(format!(
            "Saved, but re-encrypting secrets for the new SSH keys failed: {e}"
        ))
    })?;
    refresh_after_settings_change(config);
    Ok(())
}
