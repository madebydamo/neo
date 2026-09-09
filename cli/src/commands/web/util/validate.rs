/// Safe identifier for systemd units, docker names, and similar path segments.
/// ASCII alnum plus `-@._`, non-empty, max 256 chars.
pub fn unit_name_valid(unit: &str) -> bool {
    !unit.is_empty()
        && unit.len() <= 256
        && unit
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-@._".contains(c))
}

/// Activation / update / generation-switch operation ids under the ops dir.
/// Must match `activation_*`, `update_*`, or `genswitch_*` with safe path characters.
pub fn activation_id_ok(id: &str) -> bool {
    if id.is_empty() || id.len() > 200 {
        return false;
    }
    if !(id.starts_with("activation_") || id.starts_with("update_") || id.starts_with("genswitch_"))
    {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Store-repair job ids (`repair_*`) under the same ops dir.
pub fn repair_id_ok(id: &str) -> bool {
    if id.is_empty() || id.len() > 200 {
        return false;
    }
    if !id.starts_with("repair_") {
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
    br.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
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
            | "plugins"
            | "neo-cli"
            | "disko"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_id_rejects_traversal() {
        assert!(!activation_id_ok("../etc/passwd"));
        assert!(!activation_id_ok("activation_/../x"));
        assert!(activation_id_ok("activation_20240101_120000"));
        assert!(activation_id_ok("update_20240101_120000"));
        assert!(activation_id_ok("genswitch_20240101-120000"));
        assert!(!activation_id_ok("genswitch_/../x"));
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
