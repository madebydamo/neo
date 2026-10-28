//! Settings → Secrets card: where secret settings live and the switch between
//! inline → secrets.yaml → encrypted (docs/design/secrets.md). The operations
//! are the ones `neo secrets …` runs (utils/secrets_store.rs); changes apply
//! with the next activation.

use std::path::Path;
use std::sync::Arc;

use rocket::response::content::RawHtml;
use rocket::{get, post, State};
use toml_edit::DocumentMut;

use crate::commands::web::locks::{lock_attr, try_lock, Blocked, LockMode, Scope};
use crate::commands::web::settings::save::refresh_after_settings_change;
use crate::commands::web::types::AppConfig;
use crate::commands::web::util::escape_html;
use crate::utils::locks::{LockSpec, OpInfo};
use crate::utils::secrets::{Backend, PLAIN_FILE, SOPS_FILE};
use crate::utils::secrets_store::{admin_keys, host_key, ignored_keys, Store, IDENTITY_FILE};

const LOCK_ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5" aria-hidden="true"><rect x="4.5" y="10.5" width="15" height="10" rx="2"/><path d="M8 10.5V7.5a4 4 0 018 0v3"/></svg>"#;

fn store(config: &AppConfig) -> Store {
    Store::new(config.settings_path.parent().unwrap_or(Path::new(".")))
}

fn load_doc(config: &AppConfig) -> DocumentMut {
    std::fs::read_to_string(&config.settings_path)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_default()
}

/// Short display form of a public key: type, …last 12 chars, comment.
fn short_key(key: &str) -> String {
    let mut parts = key.split_whitespace();
    let kind = parts.next().unwrap_or("");
    let b64 = parts.next().unwrap_or("");
    let comment: Vec<&str> = parts.collect();
    let tail = &b64[b64.len().saturating_sub(12)..];
    format!("{kind} …{tail} {}", comment.join(" "))
        .trim()
        .to_string()
}

/// Authorized keys with their comments, ed25519 only (the admins).
fn admin_display(doc: &DocumentMut) -> Vec<String> {
    let admins = admin_keys(doc);
    doc.get("core")
        .and_then(|c| c.get("ssh"))
        .and_then(|s| s.get("authorizedKeys"))
        .and_then(|k| k.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .filter(|k| {
                    let mut p = k.split_whitespace();
                    let norm = format!("{} {}", p.next().unwrap_or(""), p.next().unwrap_or(""));
                    admins.contains(&norm)
                })
                .map(short_key)
                .collect()
        })
        .unwrap_or_default()
}

fn button(action: &str, label: &str, class: &str, confirm: Option<&str>) -> String {
    let lock = lock_attr(&[(Scope::System, LockMode::Shared)]);
    let confirm = confirm
        .map(|c| format!(r#" hx-confirm="{}""#, escape_html(c)))
        .unwrap_or_default();
    format!(
        r##"<button type="button" class="btn btn-sm {class}" hx-post="/secrets/{action}" hx-target="#secrets-card" hx-swap="outerHTML" data-neo-progress{lock}{confirm}>{label}</button>"##
    )
}

fn card(config: &AppConfig, message: Option<(&str, &str)>) -> String {
    let doc = load_doc(config);
    let store = store(config);
    let backend = store.backend();
    let admins = admin_display(&doc);
    let ignored = ignored_keys(&doc);
    let host = host_key(&doc);
    let escrowed = store.dir.join(IDENTITY_FILE).exists();

    let (badge, summary, actions) = match &backend {
        Backend::Inline => (
            r#"<span class="badge badge-sm badge-ghost">in settings.toml</span>"#,
            "Secret settings (passwords, tokens, keys) are stored in plain text inside settings.toml.".to_string(),
            [
                button("split", "Move to secrets.yaml", "btn-ghost", None),
                button("enable", "Encrypt", "btn-primary", Some("Encrypt all secret settings for this machine and the SSH keys listed below? Activate afterwards to apply.")),
            ]
            .join(""),
        ),
        Backend::Plain(_) => (
            r#"<span class="badge badge-sm badge-warning badge-soft">secrets.yaml · plain text</span>"#,
            format!("Secret settings are kept apart in {PLAIN_FILE}, still unencrypted: keep the config repository private."),
            button("enable", "Encrypt", "btn-primary", Some("Encrypt all secret settings for this machine and the SSH keys listed below? Activate afterwards to apply.")),
        ),
        Backend::Sops(_) => (
            r#"<span class="badge badge-sm badge-success badge-soft">encrypted</span>"#,
            format!(
                "Secret settings are encrypted in {SOPS_FILE}. This machine decrypts them with its SSH host key; every ssh-ed25519 key below can decrypt them too.{}",
                if escrowed {
                    format!(" The host key is escrowed for those keys in {IDENTITY_FILE}, so the machine can be restored from the repository.")
                } else {
                    String::new()
                }
            ),
            [
                button("rekey", "Re-encrypt", "btn-ghost", None),
                button("disable", "Decrypt", "btn-soft btn-warning", Some("Decrypt the secrets back into a plain-text secrets.yaml? Activate afterwards to apply.")),
            ]
            .join(""),
        ),
    };

    let admin_list = if admins.is_empty() {
        r#"<li class="text-warning">No ssh-ed25519 key in System → SSH: add one before encrypting, or nobody could restore the secrets.</li>"#.to_string()
    } else {
        admins
            .iter()
            .map(|k| format!(r#"<li class="font-mono break-all">{}</li>"#, escape_html(k)))
            .collect()
    };
    let ignored_note = if ignored.is_empty() {
        String::new()
    } else {
        format!(
            r#"<p class="text-xs text-base-content/50 mt-1">{} other key(s) can log in but not decrypt (only ssh-ed25519 is supported).</p>"#,
            ignored.len()
        )
    };
    let host_line = match host {
        Some(h) => format!(
            r#"<li class="font-mono break-all">{} <span class="text-base-content/45">(this machine)</span></li>"#,
            escape_html(&short_key(&h))
        ),
        None => String::new(),
    };
    let message = message
        .map(|(kind, text)| {
            format!(
                r#"<div role="alert" class="alert alert-{kind} alert-soft text-xs mt-3 py-2">{}</div>"#,
                escape_html(text)
            )
        })
        .unwrap_or_default();

    format!(
        r##"<div id="secrets-card" class="rounded-box border border-base-300 bg-base-100 p-4">
  <div class="flex flex-col sm:flex-row sm:items-center gap-3">
    <span class="w-9 h-9 shrink-0 rounded-xl bg-base-200 text-base-content/70 flex items-center justify-center">{LOCK_ICON}</span>
    <div class="min-w-0 flex-1">
      <div class="font-semibold text-sm flex items-center gap-2 flex-wrap">Secrets {badge}</div>
      <div class="text-xs text-base-content/60">{summary}</div>
    </div>
    <div class="flex items-center gap-1.5 shrink-0">{actions}</div>
  </div>
  <div class="mt-3 text-xs">
    <div class="text-base-content/60 mb-1">Can decrypt:</div>
    <ul class="space-y-0.5 text-[11px]">{host_line}{admin_list}</ul>
    {ignored_note}
  </div>
  {message}
</div>"##,
        summary = escape_html(&summary)
    )
}

/// GET /secrets/card — the Settings card.
#[get("/secrets/card")]
pub fn secrets_card(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    RawHtml(card(config, None))
}

fn lock(label: &str) -> Result<crate::utils::locks::LockGuard, Blocked> {
    try_lock(&LockSpec::system_shared(), OpInfo::new("secrets", label))
}

async fn secret_paths(config: &AppConfig) -> Result<Vec<Vec<String>>, String> {
    let mut ev = config.evaluator.lock().await;
    ev.extract_secret_paths()
        .await
        .map_err(|e| format!("Could not list secret settings: {e:#}"))
}

fn done(config: &AppConfig, result: Result<String, String>) -> RawHtml<String> {
    let html = match result {
        Ok(msg) => {
            refresh_after_settings_change(config);
            card(config, Some(("success", &msg)))
        }
        Err(e) => card(config, Some(("error", &e))),
    };
    RawHtml(html)
}

/// POST /secrets/split — settings.toml → secrets.yaml.
#[post("/secrets/split")]
pub async fn secrets_split(config: &State<Arc<AppConfig>>) -> Result<RawHtml<String>, Blocked> {
    let _lock = lock("Moving secrets to secrets.yaml")?;
    let result = match secret_paths(config).await {
        Ok(p) => store(config)
            .split(&p)
            .map(|moved| {
                format!(
                    "Moved {} secret setting(s) to {PLAIN_FILE}. Activate to apply.",
                    moved.len()
                )
            })
            .map_err(|e| format!("{e:#}")),
        Err(e) => Err(e),
    };
    Ok(done(config, result))
}

/// POST /secrets/enable — encrypt (host key + ssh-ed25519 authorized keys).
#[post("/secrets/enable")]
pub async fn secrets_enable(config: &State<Arc<AppConfig>>) -> Result<RawHtml<String>, Blocked> {
    let _lock = lock("Encrypting secrets")?;
    let result = match secret_paths(config).await {
        Ok(p) => store(config)
            .enable(&p)
            .map(|r| {
                format!(
                    "Encrypted {} secret(s) for {} key(s){}. Activate to apply.",
                    r.values,
                    r.recipients,
                    if r.escrowed {
                        ", host key escrowed"
                    } else {
                        ""
                    }
                )
            })
            .map_err(|e| format!("{e:#}")),
        Err(e) => Err(e),
    };
    Ok(done(config, result))
}

/// POST /secrets/disable — back to a plain secrets.yaml.
#[post("/secrets/disable")]
pub fn secrets_disable(config: &State<Arc<AppConfig>>) -> Result<RawHtml<String>, Blocked> {
    let _lock = lock("Decrypting secrets")?;
    let result = store(config)
        .disable()
        .map(|n| format!("Decrypted {n} secret(s) into {PLAIN_FILE}. Activate to apply."))
        .map_err(|e| format!("{e:#}"));
    Ok(done(config, result))
}

/// POST /secrets/rekey — re-encrypt for the current keys.
#[post("/secrets/rekey")]
pub fn secrets_rekey(config: &State<Arc<AppConfig>>) -> Result<RawHtml<String>, Blocked> {
    let _lock = lock("Re-encrypting secrets")?;
    let result = store(config)
        .rekey()
        .map(|n| format!("Re-encrypted for {n} key(s). Activate to apply."))
        .map_err(|e| format!("{e:#}"));
    Ok(done(config, result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_key_keeps_type_tail_and_comment() {
        assert_eq!(
            short_key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAQItBeU me@laptop"),
            "ssh-ed25519 …AAAAIAQItBeU me@laptop"
        );
    }

    #[test]
    fn admin_display_lists_only_ed25519() {
        let doc: DocumentMut =
            "[core.ssh]\nauthorizedKeys = [\"ssh-ed25519 AAAAone me\", \"ssh-rsa BBBB you\"]\n"
                .parse()
                .unwrap();
        let shown = admin_display(&doc);
        assert_eq!(shown.len(), 1);
        assert!(shown[0].contains("me"));
    }
}
