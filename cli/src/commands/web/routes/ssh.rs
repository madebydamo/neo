//! Homeserver SSH public key endpoints (generated at activation if missing).

use std::fs;
use std::path::Path;

use rocket::get;
use rocket::http::{ContentType, Status};
use rocket::response::content::RawHtml;
use rocket::serde::json::Json;
use serde::Serialize;

use crate::commands::web::util::escape_html;

const PUB_KEY_PATH: &str = "/home/homeserver/.ssh/id_ed25519.pub";

#[derive(Serialize)]
pub struct SshPublicKeyResponse {
    pub public_key: String,
    pub path: String,
}

fn read_public_key() -> Result<String, String> {
    let path = Path::new(PUB_KEY_PATH);
    if !path.is_file() {
        return Err(format!(
            "SSH public key not found at {PUB_KEY_PATH} (activation may not have run yet)"
        ));
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("read {PUB_KEY_PATH}: {e}"))?;
    let key = raw.trim().to_string();
    if key.is_empty() {
        return Err("SSH public key file is empty".into());
    }
    Ok(key)
}

/// GET /ssh/public-key — JSON `{ "public_key": "...", "path": "..." }`.
#[get("/ssh/public-key")]
pub fn ssh_public_key() -> Result<Json<SshPublicKeyResponse>, (Status, String)> {
    match read_public_key() {
        Ok(public_key) => Ok(Json(SshPublicKeyResponse {
            public_key,
            path: PUB_KEY_PATH.to_string(),
        })),
        Err(msg) => Err((Status::NotFound, msg)),
    }
}

/// GET /ssh/public-key.txt — raw public key line for curl/copy.
#[get("/ssh/public-key.txt")]
pub fn ssh_public_key_txt() -> Result<(ContentType, String), (Status, String)> {
    match read_public_key() {
        Ok(public_key) => Ok((ContentType::Plain, format!("{public_key}\n"))),
        Err(msg) => Err((Status::NotFound, msg)),
    }
}

/// GET /ssh/public-key-card — HTML fragment for the config UI.
#[get("/ssh/public-key-card")]
pub fn ssh_public_key_card() -> RawHtml<String> {
    match read_public_key() {
        Ok(key) => {
            let escaped = escape_html(&key);
            RawHtml(format!(
                r#"<div id="ssh-pubkey-card" class="card bg-base-100 shadow-sm border border-base-300 p-3 col-span-full">
  <div class="flex flex-wrap items-start justify-between gap-2 mb-1">
    <div>
      <div class="text-sm font-semibold">Homeserver SSH public key</div>
      <div class="text-[10px] opacity-50 font-mono">{path}</div>
    </div>
    <button type="button" class="btn btn-xs btn-ghost"
      onclick="navigator.clipboard.writeText(document.getElementById('ssh-pubkey-value').textContent.trim())">
      Copy
    </button>
  </div>
  <pre id="ssh-pubkey-value" class="text-[10px] font-mono bg-base-300 p-2 rounded overflow-x-auto whitespace-pre-wrap break-all">{escaped}</pre>
</div>"#,
                path = escape_html(PUB_KEY_PATH),
            ))
        }
        Err(msg) => RawHtml(format!(
            r#"<div id="ssh-pubkey-card" class="card bg-base-100 shadow-sm border border-warning p-3 col-span-full">
  <div class="text-sm font-semibold text-warning">Homeserver SSH public key</div>
  <p class="text-xs opacity-70 mt-1">{}</p>
  <p class="text-[10px] opacity-50 mt-1">Expected at <span class="font-mono">{}</span> after activation.</p>
</div>"#,
            escape_html(&msg),
            escape_html(PUB_KEY_PATH),
        )),
    }
}
