//! SSH public key cards: the homeserver key and the machine git key
//! (both generated at activation if missing).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use rocket::response::content::RawHtml;
use rocket::{get, post};

use crate::commands::web::util::{escape_html, sudo_cmd};

/// One keypair the UI shows and can rotate.
struct KeySpec {
    /// URL segment and DOM id suffix.
    id: &'static str,
    title: &'static str,
    hint: &'static str,
    confirm: &'static str,
    key_path: &'static str,
    /// Installed by Nix; activation runs the same script with `ensure`.
    script: &'static str,
    /// Env override for the script path (dev / tests).
    script_env: &'static str,
    /// Root-owned key: rotate through `sudo -n`.
    sudo: bool,
}

const KEYS: &[KeySpec] = &[
    KeySpec {
        id: "homeserver",
        title: "Homeserver SSH public key",
        hint: "Authorize this key on remotes (e.g. backup targets).",
        confirm: "Rotate the homeserver SSH key? Remotes (e.g. backup) must be re-authorized with the new public key.",
        key_path: "/home/homeserver/.ssh/id_ed25519",
        script: "/run/current-system/sw/bin/neo-homeserver-ssh-key",
        script_env: "NEO_HOMESERVER_SSH_KEY_CMD",
        sudo: false,
    },
    KeySpec {
        id: "git",
        title: "Git SSH public key",
        hint: "Add this key to your git forge (account SSH key or per-repo deploy key) for private plugins and Hermes. Hosts: core.git.knownHosts.",
        confirm: "Rotate the git SSH key? Forges must be re-authorized with the new public key.",
        key_path: "/var/lib/neo/git/id_ed25519",
        script: "/run/current-system/sw/bin/neo-git-ssh-key",
        script_env: "NEO_GIT_SSH_KEY_CMD",
        sudo: true,
    },
];

fn spec(id: &str) -> Option<&'static KeySpec> {
    KEYS.iter().find(|k| k.id == id)
}

/// Key actually in use: core.git.useHostKey points git at a copy of the host
/// key, published in /etc/neo/git/key (nix/modules/core/git.nix).
fn key_path(spec: &KeySpec) -> String {
    if spec.id == "git" {
        let marker =
            std::env::var("NEO_GIT_KEY_FILE").unwrap_or_else(|_| "/etc/neo/git/key".into());
        if let Ok(p) = fs::read_to_string(marker) {
            let p = p.trim();
            if !p.is_empty() {
                return p.to_string();
            }
        }
    }
    spec.key_path.to_string()
}

/// The git key is the machine host key (no separate rotation).
fn uses_host_key(spec: &KeySpec) -> bool {
    key_path(spec) != spec.key_path
}

fn pub_path(spec: &KeySpec) -> String {
    format!("{}.pub", key_path(spec))
}

fn read_public_key(spec: &KeySpec) -> Result<String, String> {
    let pub_path = pub_path(spec);
    let path = Path::new(&pub_path);
    if !path.is_file() {
        return Err(format!(
            "SSH public key not found at {pub_path} (activation may not have run yet)"
        ));
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("read {pub_path}: {e}"))?;
    let key = raw.trim().to_string();
    if key.is_empty() {
        return Err("SSH public key file is empty".into());
    }
    Ok(key)
}

/// Run the key script with `rotate` (rm + ensure), as activation would.
fn rotate_key(spec: &KeySpec) -> Result<String, String> {
    if uses_host_key(spec) {
        return Err("git uses the machine host key (core.git.useHostKey); turn that off for a separate, rotatable key".into());
    }
    let script = std::env::var(spec.script_env).unwrap_or_else(|_| spec.script.to_string());
    if Path::new(&script).is_file() {
        let out = if spec.sudo {
            Command::new(sudo_cmd())
                .args(["-n", &script, "rotate"])
                .output()
        } else {
            Command::new(&script).arg("rotate").output()
        }
        .map_err(|e| format!("run {script} rotate: {e}"))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            return Err(format!(
                "{script} rotate failed ({}): {}{}",
                out.status,
                stderr.trim(),
                if stdout.trim().is_empty() {
                    String::new()
                } else {
                    format!(" / {}", stdout.trim())
                }
            ));
        }
        return read_public_key(spec);
    }
    if spec.sudo {
        return Err(format!("{script} not found; activate first"));
    }

    // Fallback when the script is not on the running system yet (dev / pre-activate).
    let pub_path = pub_path(spec);
    let _ = fs::remove_file(spec.key_path);
    let _ = fs::remove_file(&pub_path);
    if let Some(parent) = Path::new(spec.key_path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let out = Command::new("ssh-keygen")
        .args([
            "-t",
            "ed25519",
            "-N",
            "",
            "-f",
            spec.key_path,
            "-C",
            spec.id,
        ])
        .output()
        .map_err(|e| {
            format!("ssh-keygen failed ({e}); install/activate so {script} is available")
        })?;
    if !out.status.success() {
        return Err(format!(
            "ssh-keygen failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let _ = fs::set_permissions(spec.key_path, fs::Permissions::from_mode(0o600));
    let _ = fs::set_permissions(&pub_path, fs::Permissions::from_mode(0o644));
    read_public_key(spec)
}

const KEY_ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5" aria-hidden="true"><circle cx="8" cy="15" r="4"/><path d="M11 12l8.5-8.5M16.5 7l2.5 2.5M14.5 9l2 2"/></svg>"#;

fn card_ok(spec: &KeySpec, key: &str) -> String {
    let escaped = escape_html(key);
    let path = escape_html(&pub_path(spec));
    let title = escape_html(spec.title);
    let hint = if uses_host_key(spec) {
        escape_html("This is the machine's SSH host key (core.git.useHostKey): add it to your git forge. It also decrypts the secrets, so it is not rotated here.")
    } else {
        escape_html(spec.hint)
    };
    let confirm = escape_html(spec.confirm);
    let id = spec.id;
    format!(
        r##"<div id="ssh-pubkey-card-{id}" class="rounded-box border border-base-300 bg-base-100 p-4">
  <div class="flex flex-col sm:flex-row sm:items-center gap-3">
    <span class="w-9 h-9 shrink-0 rounded-xl bg-base-200 text-base-content/70 flex items-center justify-center">{KEY_ICON}</span>
    <div class="min-w-0 flex-1">
      <div class="font-semibold text-sm">{title}</div>
      <div class="text-xs text-base-content/60">{hint} <span class="font-mono text-base-content/45 break-all">{path}</span></div>
    </div>
    <div class="flex items-center gap-1.5 shrink-0">
      <button type="button" class="btn btn-sm btn-ghost"
        onclick="var b=this;navigator.clipboard.writeText(document.getElementById('ssh-pubkey-value-{id}').textContent.trim()).then(function(){{b.textContent='Copied';setTimeout(function(){{b.textContent='Copy'}},1500)}})">Copy</button>
      <button type="button" class="btn btn-sm btn-soft btn-warning"
        hx-post="/ssh/{id}/regenerate"
        hx-target="#ssh-pubkey-card-{id}"
        hx-swap="outerHTML"
        hx-confirm="{confirm}">
        Regenerate
      </button>
    </div>
  </div>
  <pre id="ssh-pubkey-value-{id}" class="mt-3 text-[11px] leading-relaxed font-mono bg-base-200 border border-base-300 px-3 py-2 rounded-field overflow-x-auto whitespace-pre-wrap break-all">{escaped}</pre>
</div>"##
    )
}

fn card_err(spec: &KeySpec, msg: &str) -> String {
    let msg = escape_html(msg);
    let path = escape_html(&pub_path(spec));
    let title = escape_html(spec.title);
    let id = spec.id;
    format!(
        r##"<div id="ssh-pubkey-card-{id}" class="rounded-box border border-warning/40 bg-warning/5 p-4">
  <div class="flex flex-col sm:flex-row sm:items-center gap-3">
    <span class="w-9 h-9 shrink-0 rounded-xl bg-warning/15 text-warning flex items-center justify-center">{KEY_ICON}</span>
    <div class="min-w-0 flex-1">
      <div class="font-semibold text-sm">{title}</div>
      <p class="text-xs text-base-content/70 mt-0.5">{msg}</p>
      <p class="text-xs text-base-content/50 mt-0.5">Expected at <span class="font-mono">{path}</span> after activation.</p>
    </div>
    <button type="button" class="btn btn-sm btn-primary shrink-0"
      hx-post="/ssh/{id}/regenerate"
      hx-target="#ssh-pubkey-card-{id}"
      hx-swap="outerHTML">
      Generate
    </button>
  </div>
</div>"##
    )
}

/// GET /ssh/<key>/public-key-card — HTML fragment for the config UI.
#[get("/ssh/<key>/public-key-card")]
pub fn ssh_public_key_card(key: &str) -> Option<RawHtml<String>> {
    let spec = spec(key)?;
    Some(RawHtml(match read_public_key(spec) {
        Ok(k) => card_ok(spec, &k),
        Err(msg) => card_err(spec, &msg),
    }))
}

/// POST /ssh/<key>/regenerate — delete the keypair and re-run its key script.
#[post("/ssh/<key>/regenerate")]
pub fn ssh_regenerate(key: &str) -> Option<RawHtml<String>> {
    let spec = spec(key)?;
    Some(RawHtml(match rotate_key(spec) {
        Ok(k) => card_ok(spec, &k),
        Err(msg) => card_err(spec, &format!("Regenerate failed: {msg}")),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_ids_resolve_and_unknown_is_rejected() {
        assert_eq!(
            spec("git").map(|s| s.key_path),
            Some("/var/lib/neo/git/id_ed25519")
        );
        assert!(spec("homeserver").is_some_and(|s| !s.sudo));
        assert!(spec("../etc").is_none());
    }

    #[test]
    fn cards_use_per_key_dom_ids() {
        let git = spec("git").unwrap();
        let html = card_ok(git, "ssh-ed25519 AAAA neo-git@x");
        assert!(html.contains(r#"id="ssh-pubkey-card-git""#));
        assert!(html.contains(r#"hx-post="/ssh/git/regenerate""#));
        assert!(html.contains("ssh-pubkey-value-git"));
    }
}
