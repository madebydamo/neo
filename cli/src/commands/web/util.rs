use std::convert::Infallible;
use std::path::PathBuf;

use rocket::request::{FromRequest, Outcome, Request};

use super::structs::AppConfig;

pub fn config_dir(cfg: &AppConfig) -> PathBuf {
    cfg.settings_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Escape text for HTML body/content contexts (`&`, `<`, `>`, `"`, `'`).
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape a string for embedding inside a double-quoted HTML attribute.
pub fn escape_attr(s: &str) -> String {
    escape_html(s)
}

/// Escape a value for embedding inside a double-quoted Nix string literal.
pub fn escape_nix_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('$', "\\$")
}

pub fn sudo_cmd() -> String {
    std::env::var("SUDO_BINARY_PATH").unwrap_or_else(|_| "sudo".to_string())
}

pub fn nix_bin() -> String {
    std::env::var("NIX_BINARY_PATH")
        .unwrap_or_else(|_| "/run/current-system/sw/bin/nix".to_string())
}

pub fn neo_bin() -> String {
    std::env::var("NEO_BINARY_PATH")
        .unwrap_or_else(|_| "/run/current-system/sw/bin/neo".to_string())
}

/// Safe identifier for systemd units, docker names, and similar path segments.
/// ASCII alnum plus `-@._`, non-empty, max 256 chars.
pub fn unit_name_valid(unit: &str) -> bool {
    !unit.is_empty()
        && unit.len() <= 256
        && unit
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-@._".contains(c))
}

/// Activation / update operation ids used as filenames under the ops dir.
/// Must match `activation_*` or `update_*` and contain only safe path characters.
pub fn activation_id_ok(id: &str) -> bool {
    if id.is_empty() || id.len() > 200 {
        return false;
    }
    if !(id.starts_with("activation_") || id.starts_with("update_")) {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Git branch names allowed for web UI switch (activation history only).
pub fn branch_ok(br: &str) -> bool {
    if br.is_empty() || br.len() > 200 {
        return false;
    }
    if !br.starts_with("activation_") {
        return false;
    }
    // No path separators or git-special characters that could confuse switch.
    br.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Neo service names (settings.toml `[services.<name>]` keys).
pub fn service_name_ok(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 || name == "service" {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Known core / top-level settings section names accepted by save-core.
pub fn core_section_ok(section: &str) -> bool {
    matches!(
        section,
        "ssh"
            | "volumes"
            | "timeZone"
            | "uid"
            | "gid"
            | "hostname"
            | "hashedLinuxPassword"
            | "core"
            | "neo-service"
            | "neo-cli"
            | "disko"
    )
}

/// DaisyUI alert fragment kinds.
#[derive(Clone, Copy, Debug)]
pub enum AlertKind {
    Success,
    Error,
    Info,
    Warning,
}

impl AlertKind {
    fn class(self) -> &'static str {
        match self {
            AlertKind::Success => "alert-success",
            AlertKind::Error => "alert-error",
            AlertKind::Info => "alert-info",
            AlertKind::Warning => "alert-warning",
        }
    }
}

/// Build a small daisyUI alert; `msg` is HTML-escaped.
pub fn alert_html(kind: AlertKind, msg: &str) -> String {
    format!(
        r#"<div class="alert {} text-sm">{}</div>"#,
        kind.class(),
        escape_html(msg)
    )
}

/// Shared Revert / Apply button row for the changes dialog.
pub fn changes_actions_row() -> &'static str {
    r#"<div class="mt-4 flex flex-nowrap items-center justify-end gap-2" data-dialog-actions>
  <button type="button" hx-post="/changes/revert" hx-target="#changes-body" hx-swap="innerHTML" class="btn btn-sm btn-ghost">Revert</button>
  <button type="button" hx-post="/changes/apply" hx-target="#changes-body" hx-swap="innerHTML" hx-confirm="Run full activation (write-flake + nixos-rebuild)? This can take several minutes." class="btn btn-sm btn-error">Apply (activate)</button>
</div>"#
}

/// True when the client sent `HX-Request: true` (HTMX AJAX / partial load).
pub struct Htmx(pub bool);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Htmx {
    type Error = Infallible;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let is_htmx = req
            .headers()
            .get_one("HX-Request")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        Outcome::Success(Htmx(is_htmx))
    }
}

impl Htmx {
    pub fn is_htmx(&self) -> bool {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_html_quotes() {
        assert_eq!(
            escape_html(r#"a&b<"c">'d"#),
            "a&amp;b&lt;&quot;c&quot;&gt;&#39;d"
        );
    }

    #[test]
    fn activation_id_rejects_traversal() {
        assert!(!activation_id_ok("../etc/passwd"));
        assert!(!activation_id_ok("activation_/../x"));
        assert!(activation_id_ok("activation_20240101_120000"));
        assert!(activation_id_ok("update_20240101_120000"));
    }

    #[test]
    fn branch_ok_only_activation() {
        assert!(branch_ok("activation_abc"));
        assert!(!branch_ok("main"));
        assert!(!branch_ok("activation_a/b"));
    }

    #[test]
    fn service_name_rejects_fallback() {
        assert!(!service_name_ok("service"));
        assert!(service_name_ok("jellyfin"));
    }
}
