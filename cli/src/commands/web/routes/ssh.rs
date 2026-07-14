//! Homeserver SSH public key endpoints (generated at activation if missing).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use rocket::response::content::RawHtml;
use rocket::{get, post};

use crate::commands::web::util::escape_html;

const PUB_KEY_PATH: &str = "/home/homeserver/.ssh/id_ed25519.pub";
const KEY_PATH: &str = "/home/homeserver/.ssh/id_ed25519";
/// Installed by base.nix; same binary activation uses for `ensure`.
const ENSURE_CMD: &str = "/run/current-system/sw/bin/neo-homeserver-ssh-key";

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

fn ensure_cmd() -> String {
    std::env::var("NEO_HOMESERVER_SSH_KEY_CMD").unwrap_or_else(|_| ENSURE_CMD.to_string())
}

/// Remove keypair then run the same script activation uses (`rotate` = rm + ensure).
fn rotate_key() -> Result<String, String> {
    let cmd = ensure_cmd();
    // Prefer the shared script when present (post-activation).
    if Path::new(&cmd).is_file() {
        let out = Command::new(&cmd)
            .arg("rotate")
            .output()
            .map_err(|e| format!("run {cmd} rotate: {e}"))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            return Err(format!(
                "neo-homeserver-ssh-key rotate failed ({}): {}{}",
                out.status,
                stderr.trim(),
                if stdout.trim().is_empty() {
                    String::new()
                } else {
                    format!(" / {}", stdout.trim())
                }
            ));
        }
        return read_public_key();
    }

    // Fallback when the script is not on the running system yet (dev / pre-activate).
    let _ = fs::remove_file(KEY_PATH);
    let _ = fs::remove_file(PUB_KEY_PATH);
    if let Some(parent) = Path::new(KEY_PATH).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let out = Command::new("ssh-keygen")
        .args([
            "-t",
            "ed25519",
            "-N",
            "",
            "-f",
            KEY_PATH,
            "-C",
            "homeserver",
        ])
        .output()
        .map_err(|e| {
            format!("ssh-keygen failed ({e}); install/activate so {ENSURE_CMD} is available")
        })?;
    if !out.status.success() {
        return Err(format!(
            "ssh-keygen failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let _ = fs::set_permissions(KEY_PATH, fs::Permissions::from_mode(0o600));
    let _ = fs::set_permissions(PUB_KEY_PATH, fs::Permissions::from_mode(0o644));
    read_public_key()
}

fn card_ok(key: &str) -> String {
    let escaped = escape_html(key);
    let path = escape_html(PUB_KEY_PATH);
    format!(
        r##"<div id="ssh-pubkey-card" class="card bg-base-100 shadow-sm border border-base-300 p-3 col-span-full">
  <div class="flex flex-wrap items-start justify-between gap-2 mb-1">
    <div>
      <div class="text-sm font-semibold">Homeserver SSH public key</div>
      <div class="text-[10px] opacity-50 font-mono">{path}</div>
    </div>
    <div class="flex items-center gap-1">
      <button type="button" class="btn btn-xs btn-ghost"
        onclick="navigator.clipboard.writeText(document.getElementById('ssh-pubkey-value').textContent.trim())">
        Copy
      </button>
      <button type="button" class="btn btn-xs btn-warning"
        hx-post="/ssh/regenerate"
        hx-target="#ssh-pubkey-card"
        hx-swap="outerHTML"
        hx-confirm="Rotate the homeserver SSH key? Remotes (e.g. backup) must be re-authorized with the new public key.">
        Regenerate
      </button>
    </div>
  </div>
  <pre id="ssh-pubkey-value" class="text-[10px] font-mono bg-base-300 p-2 rounded overflow-x-auto whitespace-pre-wrap break-all">{escaped}</pre>
</div>"##
    )
}

fn card_err(msg: &str) -> String {
    let msg = escape_html(msg);
    let path = escape_html(PUB_KEY_PATH);
    format!(
        r##"<div id="ssh-pubkey-card" class="card bg-base-100 shadow-sm border border-warning p-3 col-span-full">
  <div class="flex flex-wrap items-start justify-between gap-2 mb-1">
    <div>
      <div class="text-sm font-semibold text-warning">Homeserver SSH public key</div>
      <p class="text-xs opacity-70 mt-1">{msg}</p>
      <p class="text-[10px] opacity-50 mt-1">Expected at <span class="font-mono">{path}</span> after activation.</p>
    </div>
    <button type="button" class="btn btn-xs btn-primary"
      hx-post="/ssh/regenerate"
      hx-target="#ssh-pubkey-card"
      hx-swap="outerHTML">
      Generate
    </button>
  </div>
</div>"##
    )
}

/// GET /ssh/public-key-card — HTML fragment for the config UI.
#[get("/ssh/public-key-card")]
pub fn ssh_public_key_card() -> RawHtml<String> {
    match read_public_key() {
        Ok(key) => RawHtml(card_ok(&key)),
        Err(msg) => RawHtml(card_err(&msg)),
    }
}

/// POST /ssh/regenerate — delete keypair and re-run neo-homeserver-ssh-key rotate.
#[post("/ssh/regenerate")]
pub fn ssh_regenerate() -> RawHtml<String> {
    match rotate_key() {
        Ok(key) => RawHtml(card_ok(&key)),
        Err(msg) => RawHtml(card_err(&format!("Regenerate failed: {msg}"))),
    }
}
