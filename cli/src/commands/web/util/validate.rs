use crate::utils::ops::OperationKind;

/// Safe identifier for systemd units, docker names, and similar path segments.
/// ASCII alnum plus `-@._`, non-empty, max 256 chars.
pub fn unit_name_valid(unit: &str) -> bool {
    !unit.is_empty()
        && unit.len() <= 256
        && unit
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-@._".contains(c))
}

/// Non-empty, at most 200 chars of ASCII alnum, `_` and `-` (safe as a path segment).
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Background op ids under the ops dir: `activation_*`, `update_*`, `genswitch_*`, `repair_*`.
pub fn op_id_ok(id: &str) -> bool {
    safe_id(id) && OperationKind::from_id(id).is_some()
}

/// Git branch names of the activation history.
pub fn branch_ok(br: &str) -> bool {
    safe_id(br) && br.starts_with("activation_")
}

/// Git revision allowed for versioning APIs: `activation_*` branch name or hex SHA (7–40).
pub fn rev_ok(rev: &str) -> bool {
    if rev.is_empty() || rev.len() > 200 {
        return false;
    }
    if branch_ok(rev) {
        return true;
    }
    let len = rev.len();
    (7..=40).contains(&len) && rev.chars().all(|c| c.is_ascii_hexdigit())
}

/// NixOS generation numbers accepted by the web UI (positive, reasonable bound).
pub fn generation_ok(n: u64) -> bool {
    n > 0 && n < 1_000_000
}

/// Local unix username for sudo -u (OAuth runAs).
pub fn run_as_ok(name: &str) -> bool {
    if name.is_empty() || name.len() > 32 {
        return false;
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Provider / catalog ids (`xai`, `openai-codex`).
pub fn provider_id_ok(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 {
        return false;
    }
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// OAuth session ids from the helper (url-safe).
pub fn oauth_session_ok(id: &str) -> bool {
    if id.len() < 8 || id.len() > 64 {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Option names in the schema (may be dotted: `llm`, `vpn.enabled`).
pub fn option_name_ok(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
}

/// Env var names passed through ui.oauth.env.
pub fn oauth_env_key_ok(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Neo service names (settings.toml `[services.<name>]` keys).
pub fn service_name_ok(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 || name == "service" {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A path segment that passed [`service_name_ok`]. As a route parameter an
/// invalid name never reaches the handler: Rocket answers 422 (the UI only
/// ever sends names it got from the server, so nothing swaps on that).
#[derive(Debug, Clone, Copy)]
pub struct ServiceName<'r>(&'r str);

impl<'r> ServiceName<'r> {
    pub fn as_str(&self) -> &'r str {
        self.0
    }
}

impl std::ops::Deref for ServiceName<'_> {
    type Target = str;
    fn deref(&self) -> &str {
        self.0
    }
}

impl std::fmt::Display for ServiceName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl<'r> rocket::request::FromParam<'r> for ServiceName<'r> {
    type Error = &'r str;
    fn from_param(param: &'r str) -> Result<Self, Self::Error> {
        if service_name_ok(param) {
            Ok(ServiceName(param))
        } else {
            Err(param)
        }
    }
}

/// Settings sections stored under `[core]`: the aggregate `core` pane and its keys.
pub const CORE_NESTED_SECTIONS: &[&str] = &[
    "ssh",
    "volumes",
    "timeZone",
    "uid",
    "gid",
    "hostname",
    "hashedLinuxPassword",
    "plugins",
    "core",
];

/// Known core / top-level settings section names accepted by save-core.
pub fn core_section_ok(section: &str) -> bool {
    CORE_NESTED_SECTIONS.contains(&section) || matches!(section, "neo-cli" | "disko")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_id_rejects_traversal() {
        assert!(!op_id_ok("../etc/passwd"));
        assert!(!op_id_ok("activation_/../x"));
        assert!(op_id_ok("activation_20240101_120000"));
        assert!(op_id_ok("update_20240101_120000"));
        assert!(op_id_ok("genswitch_20240101-120000"));
        assert!(op_id_ok("repair_20240101-120000"));
        assert!(!op_id_ok("genswitch_/../x"));
        assert!(!op_id_ok("nope_1"));
    }

    #[test]
    fn branch_ok_only_activation() {
        assert!(branch_ok("activation_abc"));
        assert!(!branch_ok("main"));
        assert!(!branch_ok("activation_a/b"));
    }

    #[test]
    fn rev_ok_sha_or_activation_branch() {
        assert!(rev_ok("activation_20240101-120000"));
        assert!(rev_ok("abcdef0"));
        assert!(rev_ok("0123456789abcdef0123456789abcdef01234567"));
        assert!(!rev_ok("short"));
        assert!(!rev_ok("main"));
        assert!(!rev_ok("../etc/passwd"));
        assert!(!rev_ok("abc;rm"));
    }

    #[test]
    fn generation_ok_positive_bound() {
        assert!(generation_ok(1));
        assert!(generation_ok(42));
        assert!(!generation_ok(0));
        assert!(!generation_ok(1_000_000));
    }

    #[test]
    fn service_name_rejects_fallback() {
        assert!(!service_name_ok("service"));
        assert!(service_name_ok("jellyfin"));
        use rocket::request::FromParam;
        assert!(ServiceName::from_param("jellyfin").is_ok());
        assert!(ServiceName::from_param("../etc").is_err());
        assert!(ServiceName::from_param("service").is_err());
    }

    #[test]
    fn run_as_and_provider_ids() {
        assert!(run_as_ok("hermes"));
        assert!(!run_as_ok("hermes;id"));
        assert!(!run_as_ok("../root"));
        assert!(provider_id_ok("openai-codex"));
        assert!(provider_id_ok("xai"));
        assert!(!provider_id_ok("xai/../x"));
        assert!(!provider_id_ok("XAI"));
        assert!(oauth_session_ok("abcdefgh"));
        assert!(!oauth_session_ok("short"));
        assert!(!oauth_session_ok("../abcd"));
        assert!(option_name_ok("llm"));
        assert!(oauth_env_key_ok("HERMES_HOME"));
        assert!(!oauth_env_key_ok("PATH;rm"));
    }
}
