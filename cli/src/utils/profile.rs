//! Resolve neo-cli local/server profiles and field lookup.
//!
//! Profile selection:
//! - `--profile` / `NEO_PROFILE` wins
//! - `--section` / `NEO_SECTION` is an alias for `--profile` (`local` or `server`)
//! - else: `server` if `/etc/neo/settings.toml` exists, otherwise `local`
//!
//! Field resolution: `neo-cli.<profile>.<key>` then `neo-cli.<key>` then caller default.

use toml_edit::{DocumentMut, Item};

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
        "server" => Some(PROFILE_SERVER),
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

/// Default flake input for neo. The server profile uses this unless overridden.
pub const DEFAULT_NEO_INPUT: &str = "github:madebydamo/neo";

/// `neo.core.volumes.root` unless the settings move it.
pub const DEFAULT_VOLUMES_ROOT: &str = "/var/neo";
/// The config repo below the volumes root unless `configPath` moves it.
pub const DEFAULT_CONFIG_REL: &str = "DATA/AppData/configuration";
/// [`DEFAULT_VOLUMES_ROOT`]/[`DEFAULT_CONFIG_REL`] (checked by a test).
pub const DEFAULT_SERVER_CONFIG_PATH: &str = "/var/neo/DATA/AppData/configuration";

/// Default `nix flake init -t` template.
pub const DEFAULT_TEMPLATE: &str = "github:madebydamo/neo#homeserver";

/// Look up a string key: profile table first, then shared neo-cli.
pub fn neo_cli_get<'a>(doc: &'a DocumentMut, profile: &str, key: &str) -> Option<&'a str> {
    profile_str(doc, profile, key).or_else(|| shared_str(doc, key))
}

fn non_empty_str(item: &Item) -> Option<&str> {
    item.as_str().map(str::trim).filter(|s| !s.is_empty())
}

fn profile_str<'a>(doc: &'a DocumentMut, profile: &str, key: &str) -> Option<&'a str> {
    non_empty_str(doc.get("neo-cli")?.get(profile)?.get(key)?)
}

fn shared_str<'a>(doc: &'a DocumentMut, key: &str) -> Option<&'a str> {
    non_empty_str(doc.get("neo-cli")?.get(key)?)
}

/// A flake ref that only exists on the machine that has this checkout.
/// `git+file:`, `path:`, `file:`, and absolute or relative directories.
pub fn is_local_flake_ref(raw: &str) -> bool {
    let base = raw.trim().split('#').next().unwrap_or("").trim();
    base.starts_with('/')
        || base.starts_with("./")
        || base.starts_with("../")
        || base.starts_with("git+file:")
        || base.starts_with("path:")
        || base.starts_with("file:")
}

/// Directory form of a local flake ref, without a `git+file:` / `path:` prefix.
pub fn local_flake_dir(raw: &str) -> Option<String> {
    let base = raw.trim().split('#').next().unwrap_or("").trim();
    let path = base
        .strip_prefix("git+file://")
        .or_else(|| base.strip_prefix("git+file:"))
        .or_else(|| base.strip_prefix("path:"))
        .or_else(|| base.strip_prefix("file://"))
        .or_else(|| base.strip_prefix("file:"))
        .unwrap_or(base);
    if path.starts_with('/') || path.starts_with("./") || path.starts_with("../") {
        Some(path.to_string())
    } else {
        None
    }
}

/// `nix flake init -t` ref for a neo input. Local inputs become `<dir>#homeserver`.
pub fn template_from_neo_input(input: &str) -> String {
    let input = input.trim();
    if input.is_empty() {
        return DEFAULT_TEMPLATE.to_string();
    }
    if let Some(dir) = local_flake_dir(input) {
        return format!("{dir}#homeserver");
    }
    if input.contains('#') {
        input.to_string()
    } else {
        format!("{input}#homeserver")
    }
}

/// neoInput for the active profile.
///
/// Profile value wins. A shared value that points at a local checkout is not
/// used for the server profile; that profile falls through to GitHub.
pub fn resolve_neo_input(doc: &DocumentMut, profile: &str) -> String {
    if let Some(v) = profile_str(doc, profile, "neoInput") {
        return v.to_string();
    }
    if let Some(v) = shared_str(doc, "neoInput") {
        if !(profile == PROFILE_SERVER && is_local_flake_ref(v)) {
            return v.to_string();
        }
    }
    DEFAULT_NEO_INPUT.to_string()
}

/// Template for `nix flake init -t`.
///
/// Profile `template` wins. Otherwise a profile `neoInput` is turned into a
/// template. A shared template that is a local path is not used on the server.
pub fn resolve_template(doc: &DocumentMut, profile: &str) -> String {
    if let Some(v) = profile_str(doc, profile, "template") {
        return v.to_string();
    }
    if profile_str(doc, profile, "neoInput").is_some() {
        return template_from_neo_input(&resolve_neo_input(doc, profile));
    }
    if let Some(v) = shared_str(doc, "template") {
        if !(profile == PROFILE_SERVER && is_local_flake_ref(v)) {
            return v.to_string();
        }
    }
    template_from_neo_input(&resolve_neo_input(doc, profile))
}

/// Write one string into `neo-cli.<profile>.<key>` (CLI overrides).
pub fn set_profile_str(doc: &mut DocumentMut, profile: &str, key: &str, value: &str) {
    use toml_edit::Table;
    let cli = doc.entry("neo-cli").or_insert(Item::Table(Table::new()));
    let Some(cli_tbl) = cli.as_table_mut() else {
        return;
    };
    let prof = cli_tbl.entry(profile).or_insert(Item::Table(Table::new()));
    let Some(prof_tbl) = prof.as_table_mut() else {
        return;
    };
    prof_tbl.insert(key, toml_edit::value(value));
}

/// Resolve configPath for the active profile with sensible fallbacks.
pub fn resolve_config_path(doc: &DocumentMut, profile: &str) -> String {
    if let Some(p) = neo_cli_get(doc, profile, "configPath") {
        return p.to_string();
    }
    if profile == PROFILE_SERVER {
        DEFAULT_SERVER_CONFIG_PATH.to_string()
    } else {
        "./build".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_path_is_inside_the_default_root() {
        assert_eq!(
            DEFAULT_SERVER_CONFIG_PATH,
            format!("{DEFAULT_VOLUMES_ROOT}/{DEFAULT_CONFIG_REL}")
        );
    }

    fn doc(raw: &str) -> DocumentMut {
        raw.parse().unwrap()
    }

    #[test]
    fn server_ignores_shared_local_checkout() {
        let doc = doc(r#"
[neo-cli]
neoInput = "git+file:/home/damo/Documents/projects/homeserver/neo"
template = "/home/damo/Documents/projects/homeserver/neo#homeserver"

[neo-cli.local]
configPath = "/home/damo/Documents/projects/homeserver/neo/build"
neoInput = "git+file:/home/damo/Documents/projects/homeserver/neo"
template = "/home/damo/Documents/projects/homeserver/neo#homeserver"
"#);
        assert_eq!(resolve_neo_input(&doc, PROFILE_SERVER), DEFAULT_NEO_INPUT);
        assert_eq!(resolve_template(&doc, PROFILE_SERVER), DEFAULT_TEMPLATE);
        assert_eq!(
            resolve_neo_input(&doc, PROFILE_LOCAL),
            "git+file:/home/damo/Documents/projects/homeserver/neo"
        );
        assert_eq!(
            resolve_template(&doc, PROFILE_LOCAL),
            "/home/damo/Documents/projects/homeserver/neo#homeserver"
        );
    }

    #[test]
    fn server_neo_input_override_wins() {
        let doc = doc(r#"
[neo-cli.server]
neoInput = "github:example/neo"
"#);
        assert_eq!(
            resolve_neo_input(&doc, PROFILE_SERVER),
            "github:example/neo"
        );
        assert_eq!(
            resolve_template(&doc, PROFILE_SERVER),
            "github:example/neo#homeserver"
        );
    }

    #[test]
    fn git_file_input_becomes_directory_template() {
        assert_eq!(
            template_from_neo_input("git+file:/srv/neo"),
            "/srv/neo#homeserver"
        );
        assert_eq!(
            template_from_neo_input("github:madebydamo/neo"),
            DEFAULT_TEMPLATE
        );
    }
}
