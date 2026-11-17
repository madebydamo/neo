//! Settings → Secrets card: where secret settings live (docs/design/secrets.md),
//! who can decrypt them and the switches the backend allows. Rendered by
//! `templates/secrets_card.html.hbs`.

use serde::Serialize;
use toml_edit::DocumentMut;

use crate::commands::web::settings::save::load_settings_doc;
use crate::commands::web::types::AppConfig;
use crate::utils::secrets_store::{admin_keys, host_key, ignored_keys, IDENTITY_FILE};
use crate::utils::secrets_yaml::{Backend, PLAIN_FILE, SOPS_FILE};

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

#[derive(Serialize)]
pub struct Message {
    /// Alert kind: success, error or warning.
    kind: String,
    text: String,
}

#[derive(Serialize)]
pub struct CardContext {
    /// `inline`, `plain` or `sops`.
    backend: &'static str,
    plain_file: &'static str,
    sops_file: &'static str,
    identity_file: &'static str,
    escrowed: bool,
    /// This machine's host key (short form).
    host: Option<String>,
    admins: Vec<String>,
    /// Authorized keys that cannot decrypt (not ssh-ed25519).
    ignored: usize,
    message: Option<Message>,
}

/// The card's context; `message` is `(kind, text)` of an operation result.
pub fn context(config: &AppConfig, message: Option<(&str, &str)>) -> CardContext {
    let (doc, unreadable) = match load_settings_doc(&config.settings_path) {
        Ok(doc) => (doc, None),
        Err(e) => (DocumentMut::new(), Some(e.message().to_string())),
    };
    let store = config.secrets_store();
    let backend = store.backend();
    let stale = (backend != Backend::Inline && !store.loads_secret_files()).then(|| {
        format!(
            "This configuration's modules/settings.nix does not load {PLAIN_FILE} / {SOPS_FILE}: \
             the secrets kept there are missing from the evaluated system and saves are refused. \
             Refresh the modules from the neo homeserver template (neo update) first."
        )
    });
    let message = message
        .or(unreadable.as_deref().map(|t| ("error", t)))
        .or(stale.as_deref().map(|t| ("warning", t)))
        .map(|(kind, text)| Message {
            kind: kind.into(),
            text: text.into(),
        });
    CardContext {
        backend: match backend {
            Backend::Inline => "inline",
            Backend::Plain(_) => "plain",
            Backend::Sops(_) => "sops",
        },
        plain_file: PLAIN_FILE,
        sops_file: SOPS_FILE,
        identity_file: IDENTITY_FILE,
        escrowed: store.dir.join(IDENTITY_FILE).exists(),
        host: host_key(&doc).map(|h| short_key(&h)),
        admins: admin_display(&doc),
        ignored: ignored_keys(&doc).len(),
        message,
    }
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

    fn render(ctx: &CardContext) -> String {
        use rocket_dyn_templates::handlebars::Handlebars;
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/templates/secrets_card.html.hbs"
        ))
        .unwrap();
        let mut hb = Handlebars::new();
        hb.register_template_string("card", src).unwrap();
        hb.render("card", ctx).unwrap()
    }

    fn ctx(backend: &'static str, admins: Vec<String>) -> CardContext {
        CardContext {
            backend,
            plain_file: PLAIN_FILE,
            sops_file: SOPS_FILE,
            identity_file: IDENTITY_FILE,
            escrowed: true,
            host: Some("ssh-ed25519 …host".into()),
            admins,
            ignored: 2,
            message: Some(Message {
                kind: "success".into(),
                text: "Done <now>".into(),
            }),
        }
    }

    #[test]
    fn card_shows_the_backend_actions() {
        let html = render(&ctx("inline", vec![]));
        assert!(html.contains(r#"hx-post="/secrets/split""#));
        assert!(html.contains(r#"hx-post="/secrets/enable""#));
        assert!(html.contains(r#"hx-confirm="Encrypt all secret settings"#));
        assert!(!html.contains("/secrets/rekey"));
        assert!(html.contains("No ssh-ed25519 key"));
        assert!(html.contains("Done &lt;now&gt;"));
        assert!(html.contains(r#"alert-success"#));
        assert!(html.contains(r#"data-neo-lock="system:sh""#));
        let html = render(&ctx("sops", vec!["ssh-ed25519 …abc me".into()]));
        assert!(html.contains(r#"hx-post="/secrets/rekey""#));
        assert!(html.contains(r#"hx-post="/secrets/disable""#));
        assert!(!html.contains("/secrets/split"));
        assert!(html.contains("is escrowed for those keys"));
        assert!(html.contains("…abc me"));
        assert!(!html.contains("No ssh-ed25519 key"));
        assert!(html.contains("2 other key(s)"));
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
