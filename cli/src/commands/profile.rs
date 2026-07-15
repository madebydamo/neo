//! Resolve neo-cli local/server profiles and field lookup.
//!
//! Profile selection:
//! - `--profile` / `NEO_PROFILE` wins
//! - `--section` / `NEO_SECTION` is an alias (also accepts legacy neo-cli / neo-service)
//! - else: `server` if `/etc/neo/settings.toml` exists, otherwise `local`
//!
//! Field resolution: `neo-cli.<profile>.<key>` then `neo-cli.<key>` then caller default.

use toml_edit::DocumentMut;

pub const PROFILE_LOCAL: &str = "local";
pub const PROFILE_SERVER: &str = "server";

/// Normalize an explicit profile/section string into `local` or `server`.
/// Returns `None` if empty (caller should apply auto detection).
pub fn normalize_profile_arg(raw: &str) -> Option<&'static str> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    match s {
        "local" | "cli" | "neo-cli" => Some(PROFILE_LOCAL),
        "server" | "neo-service" | "nixos" => Some(PROFILE_SERVER),
        other => {
            // Unknown values: treat "local"/"server" case-insensitively if possible.
            let lower = other.to_ascii_lowercase();
            if lower == PROFILE_LOCAL {
                Some(PROFILE_LOCAL)
            } else if lower == PROFILE_SERVER {
                Some(PROFILE_SERVER)
            } else {
                eprintln!(
                    "warning: unknown profile/section {:?} — expected local|server (using auto)",
                    raw
                );
                None
            }
        }
    }
}

/// Pick active profile from CLI flags and environment markers.
pub fn resolve_profile(
    profile_flag: &str,
    section_flag: &str,
    etc_settings_exists: bool,
) -> String {
    if let Some(p) = normalize_profile_arg(profile_flag) {
        return p.to_string();
    }
    if let Some(p) = normalize_profile_arg(section_flag) {
        return p.to_string();
    }
    if etc_settings_exists {
        PROFILE_SERVER.to_string()
    } else {
        PROFILE_LOCAL.to_string()
    }
}

/// Look up a string key: profile table first, then shared neo-cli.
pub fn neo_cli_get<'a>(doc: &'a DocumentMut, profile: &str, key: &str) -> Option<&'a str> {
    let cli = doc.get("neo-cli")?;
    cli.get(profile)
        .and_then(|t| t.get(key))
        .and_then(|v| v.as_str())
        .or_else(|| cli.get(key).and_then(|v| v.as_str()))
}

/// Resolve configPath for the active profile with sensible fallbacks.
pub fn resolve_config_path(doc: &DocumentMut, profile: &str) -> String {
    if let Some(p) = neo_cli_get(doc, profile, "configPath") {
        if !p.is_empty() {
            return p.to_string();
        }
    }
    // Legacy top-level / old section names (pre-004 / pre-003).
    let legacy = doc
        .get("neo-cli")
        .and_then(|t| t.get("configPath"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            doc.get("neo-service")
                .or_else(|| doc.get("cli"))
                .or_else(|| doc.get("nixos"))
                .and_then(|t| t.get("configPath"))
                .and_then(|v| v.as_str())
        });
    if let Some(p) = legacy {
        if !p.is_empty() {
            return p.to_string();
        }
    }
    if profile == PROFILE_SERVER {
        "/var/neo/DATA/AppData/configuration".to_string()
    } else {
        "./build".to_string()
    }
}
