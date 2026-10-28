use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{Array, DocumentMut, Item, Table};

use crate::utils::{is_local_flake_ref, sort_document_alphabetically};

pub fn migrate(config_path: &str, source_settings: &Path, dry_run: bool) -> Result<()> {
    let live = PathBuf::from(config_path).join("settings.toml");
    let target = if live.exists() {
        live
    } else if source_settings.exists() {
        source_settings.to_path_buf()
    } else {
        live
    };
    if dry_run {
        println!("DRY-RUN: migrate {}", target.display());
        return Ok(());
    }
    if !target.exists() {
        if let Some(p) = target.parent() {
            fs::create_dir_all(p).context("create dir for new settings")?;
        }
        let mut doc = DocumentMut::new();
        apply_migrations(&mut doc);
        sort_document_alphabetically(&mut doc);
        fs::write(&target, doc.to_string()).context("write initial settings with migrations")?;
        println!("Initialized migrations marker in {}", target.display());
        return Ok(());
    }
    let content = fs::read_to_string(&target).context("read settings.toml for migration")?;
    let mut doc: DocumentMut = content.parse().context("parse settings.toml")?;
    let changed = apply_migrations(&mut doc);
    if changed {
        // Same ordering as web saves so update→migrate does not unsort the file.
        sort_document_alphabetically(&mut doc);
        fs::write(&target, doc.to_string()).context("write migrated settings.toml")?;
        println!("Migrations applied to {}", target.display());
    } else {
        println!(
            "No migrations applied (already up to date) at {}",
            target.display()
        );
    }
    Ok(())
}

/// Apply pending settings migrations in place. Returns true if the document changed.
/// Only invoked from `neo migrate` (e.g. during `neo update`), not on paste/upload/save.
fn apply_migrations(doc: &mut DocumentMut) -> bool {
    let mut applied = get_applied(doc);
    let orig = applied.len();
    // Path renames first, then migrations that need more than renames.
    let steps = MIGRATIONS
        .iter()
        .map(|m| (m.id, Step::Renames(m.renames)))
        .chain(
            CUSTOM_MIGRATIONS
                .iter()
                .map(|&(id, f)| (id, Step::Custom(f))),
        );
    for (id, step) in steps {
        if applied.iter().any(|a| a == id) {
            continue;
        }
        println!("Applying migration: {id}");
        match step {
            Step::Renames(renames) => {
                for &(from, to) in renames {
                    move_dotted(doc, from, to);
                }
            }
            Step::Custom(f) => f(doc),
        }
        applied.push(id.to_string());
    }
    let did_new = applied.len() > orig;
    if did_new {
        set_applied(doc, &applied);
    }
    did_new
}

type MigrationFn = fn(&mut DocumentMut);

enum Step {
    Renames(&'static [KeyRename]),
    Custom(MigrationFn),
}

const CUSTOM_MIGRATIONS: &[(&str, MigrationFn)] = &[
    ("003-split-neo-service", migrate_003_split_neo_service),
    (
        "004-neo-cli-local-server-profiles",
        migrate_004_neo_cli_profiles,
    ),
    ("006-hermes-unified-llm", migrate_006_hermes_unified_llm),
    (
        "007-neo-cli-profile-inputs",
        migrate_007_neo_cli_profile_inputs,
    ),
    ("008-core-git-identity", migrate_008_core_git_identity),
    (
        "009-system-updater-gc-older-than",
        migrate_009_system_updater_gc_older_than,
    ),
    (
        "010-tinyauth-oauth-providers",
        migrate_010_tinyauth_oauth_providers,
    ),
    // After 003/004, which may carry the key over from retired tables.
    (
        "011-drop-bootstrap-method",
        migrate_011_drop_bootstrap_method,
    ),
];

/// Delete retired [neo-service] and [nixos] tables.
///
/// Keys that still exist are moved to core.plugins, services.system-updater, and neo-cli.
/// Anything else in those tables (enabled, bootstrapEnabled, …) is dropped.
fn migrate_003_split_neo_service(doc: &mut DocumentMut) {
    for key in ["neo-service", "nixos"] {
        if let Some(item) = remove_dotted(doc, key) {
            absorb_retired_service_table(doc, &item);
        }
    }
}

fn absorb_retired_service_table(doc: &mut DocumentMut, svc_item: &Item) {
    let Some(svc) = svc_item.as_table() else {
        return;
    };

    if let Some(plugins) = svc.get("plugins") {
        insert_dotted(doc, "core.plugins", plugins.clone());
    }

    // system-updater.enabled from autoUpdateEnabled, else bootstrapEnabled
    if let Some(v) = svc.get("autoUpdateEnabled") {
        insert_dotted(doc, "services.system-updater.enabled", v.clone());
    } else if let Some(v) = svc.get("bootstrapEnabled") {
        insert_dotted(doc, "services.system-updater.enabled", v.clone());
    }
    if let Some(v) = svc.get("autoUpdateTimer") {
        insert_dotted(doc, "services.system-updater.schedule", v.clone());
    }
    // Legacy neo-service key kept the misspelling; write the corrected option name.
    if let Some(v) = svc.get("garbageCollectOlderThen") {
        insert_dotted(
            doc,
            "services.system-updater.garbageCollectOlderThan",
            v.clone(),
        );
    }

    // Server-side config path → neo-cli.server (004 also handles legacy top-level configPath).
    if let Some(v) = svc.get("configPath") {
        let already = doc
            .get("neo-cli")
            .and_then(|t| t.get("server"))
            .and_then(|t| t.as_table())
            .map(|t| t.contains_key("configPath"))
            .unwrap_or(false);
        if !already {
            insert_dotted(doc, "neo-cli.server.configPath", v.clone());
        }
    }

    // Shared CLI keys: fill neo-cli only when the key is not already set.
    const CLI_KEYS: &[&str] = &[
        "repoUrl",
        "neoInput",
        "template",
        "bootstrapMethod",
        "gitUserName",
        "gitUserEmail",
        "defaultBranch",
        "rebuildBranchFormat",
    ];
    for key in CLI_KEYS {
        let already = doc
            .get("neo-cli")
            .and_then(|t| t.as_table())
            .map(|t| t.contains_key(key))
            .unwrap_or(false);
        if already {
            continue;
        }
        if let Some(v) = svc.get(key) {
            insert_dotted(doc, &format!("neo-cli.{key}"), v.clone());
        }
    }
}

/// Move top-level neo-cli.configPath into local/server profile tables.
fn migrate_004_neo_cli_profiles(doc: &mut DocumentMut) {
    let path_item = remove_dotted(doc, "neo-cli.configPath");
    let path_str = path_item
        .as_ref()
        .and_then(|i| i.as_str())
        .map(|s| s.to_string());

    if let Some(path) = path_str {
        let localish = is_local_config_path(&path);
        let serverish = is_server_config_path(&path);

        let has_local = doc
            .get("neo-cli")
            .and_then(|t| t.get("local"))
            .and_then(|t| t.as_table())
            .map(|t| t.contains_key("configPath"))
            .unwrap_or(false);
        let has_server = doc
            .get("neo-cli")
            .and_then(|t| t.get("server"))
            .and_then(|t| t.as_table())
            .map(|t| t.contains_key("configPath"))
            .unwrap_or(false);

        if serverish && !localish {
            if !has_server {
                insert_dotted(
                    doc,
                    "neo-cli.server.configPath",
                    Item::Value(toml_edit::Value::from(path.as_str())),
                );
            }
        } else if localish && !serverish {
            if !has_local {
                insert_dotted(
                    doc,
                    "neo-cli.local.configPath",
                    Item::Value(toml_edit::Value::from(path.as_str())),
                );
            }
        } else {
            // Ambiguous: keep on both profiles if missing.
            if !has_local {
                insert_dotted(
                    doc,
                    "neo-cli.local.configPath",
                    Item::Value(toml_edit::Value::from(path.as_str())),
                );
            }
            if !has_server {
                insert_dotted(
                    doc,
                    "neo-cli.server.configPath",
                    Item::Value(toml_edit::Value::from(path.as_str())),
                );
            }
        }
    }
}

fn table_nonempty_str(table: &Table, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(|i| i.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn llm_has_str(hermes: &Table, key: &str) -> bool {
    hermes
        .get("llm")
        .and_then(|i| i.as_table())
        .and_then(|t| table_nonempty_str(t, key))
        .is_some()
}

/// Collapse split Hermes LLM keys into services.hermes.llm.{provider,apiKey,model}.
fn migrate_006_hermes_unified_llm(doc: &mut DocumentMut) {
    let Some(services) = doc.get_mut("services").and_then(|s| s.as_table_mut()) else {
        return;
    };
    let Some(hermes) = services.get_mut("hermes").and_then(|s| s.as_table_mut()) else {
        return;
    };

    let model_provider = table_nonempty_str(hermes, "modelProvider");
    let default_model = table_nonempty_str(hermes, "defaultModel");
    let xai = table_nonempty_str(hermes, "xaiApiKey");
    let anthropic = table_nonempty_str(hermes, "anthropicApiKey");
    let openai = table_nonempty_str(hermes, "openaiApiKey");
    let openrouter = table_nonempty_str(hermes, "openrouterApiKey");

    let derived_provider = model_provider.clone().or_else(|| {
        if xai.is_some() {
            Some("xai".into())
        } else if anthropic.is_some() {
            Some("anthropic".into())
        } else if openai.is_some() {
            Some("openai".into())
        } else if openrouter.is_some() {
            Some("openrouter".into())
        } else {
            None
        }
    });

    let derived_key = match derived_provider.as_deref() {
        Some("xai") | Some("xai-oauth") => xai.clone(),
        Some("anthropic") => anthropic.clone(),
        Some("openai") | Some("openai-api") => openai.clone(),
        Some("openrouter") => openrouter.clone(),
        _ => xai
            .clone()
            .or(anthropic.clone())
            .or(openai.clone())
            .or(openrouter.clone()),
    };

    let skip_provider = llm_has_str(hermes, "provider");
    let skip_key = llm_has_str(hermes, "apiKey");
    let skip_model = llm_has_str(hermes, "model");

    if derived_provider.is_some() || derived_key.is_some() || default_model.is_some() {
        if hermes.get("llm").and_then(|i| i.as_table()).is_none() {
            hermes.insert("llm", Item::Table(Table::new()));
        }
        if let Some(llm) = hermes.get_mut("llm").and_then(|i| i.as_table_mut()) {
            if !skip_provider {
                if let Some(p) = derived_provider {
                    llm.insert("provider", toml_edit::value(p));
                }
            }
            if !skip_key {
                if let Some(k) = derived_key {
                    llm.insert("apiKey", toml_edit::value(k));
                }
            }
            if !skip_model {
                if let Some(m) = default_model {
                    llm.insert("model", toml_edit::value(m));
                }
            }
        }
    }

    for key in [
        "xaiApiKey",
        "anthropicApiKey",
        "openaiApiKey",
        "openrouterApiKey",
        "modelProvider",
        "defaultModel",
    ] {
        hermes.remove(key);
    }

    if let Some(llm) = hermes.get("llm").and_then(|i| i.as_table()) {
        if llm.is_empty() {
            hermes.remove("llm");
        }
    }
}

/// Move shared neo-cli.neoInput / template onto the profiles.
/// A local checkout path stays on the local profile. The server profile keeps
/// the GitHub default instead of inheriting that path. A remote URL is copied
/// to both profiles.
fn migrate_007_neo_cli_profile_inputs(doc: &mut DocumentMut) {
    for key in ["neoInput", "template"] {
        let Some(raw) = doc
            .get("neo-cli")
            .and_then(|t| t.get(key))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
        else {
            continue;
        };
        if raw.is_empty() {
            remove_dotted(doc, &format!("neo-cli.{key}"));
            continue;
        }
        let item = Item::Value(toml_edit::Value::from(raw.as_str()));
        let profiles: &[&str] = if is_local_flake_ref(&raw) {
            &["local"]
        } else {
            &["local", "server"]
        };
        for profile in profiles {
            if profile_has_key(doc, profile, key) {
                continue;
            }
            insert_dotted(doc, &format!("neo-cli.{profile}.{key}"), item.clone());
        }
        remove_dotted(doc, &format!("neo-cli.{key}"));
    }
}

/// neo-cli.gitUserName/gitUserEmail → core.git.userName/userEmail (the machine
/// git identity). Custom so it runs after 003, which still moves the keys into
/// neo-cli. An existing core.git value wins.
fn migrate_008_core_git_identity(doc: &mut DocumentMut) {
    for (from, to) in [("gitUserName", "userName"), ("gitUserEmail", "userEmail")] {
        let Some(val) = remove_dotted(doc, &format!("neo-cli.{from}")) else {
            continue;
        };
        let taken = doc
            .get("core")
            .and_then(|t| t.get("git"))
            .and_then(|t| t.as_table_like())
            .is_some_and(|t| t.contains_key(to));
        if !taken {
            insert_dotted(doc, &format!("core.git.{to}"), val);
        }
    }
}

/// Fix system-updater GC option typo: OlderThen → OlderThan.
/// Existing corrected key wins; the misspelled key is always dropped.
fn migrate_009_system_updater_gc_older_than(doc: &mut DocumentMut) {
    let Some(val) = remove_dotted(doc, "services.system-updater.garbageCollectOlderThen") else {
        return;
    };
    let taken = doc
        .get("services")
        .and_then(|t| t.get("system-updater"))
        .and_then(|t| t.as_table_like())
        .is_some_and(|t| t.contains_key("garbageCollectOlderThan"));
    if !taken {
        insert_dotted(doc, "services.system-updater.garbageCollectOlderThan", val);
    }
}

/// `neo-cli.bootstrapMethod` is gone: config repos always come from the template.
fn migrate_011_drop_bootstrap_method(doc: &mut DocumentMut) {
    for path in [
        "neo-cli.bootstrapMethod",
        "neo-cli.local.bootstrapMethod",
        "neo-cli.server.bootstrapMethod",
    ] {
        remove_dotted(doc, path);
    }
}

/// Move the single tinyauth Entra block into the multi-provider `oauth` attrset:
/// `entra.*` → `oauth.entra.*` (+ `type = "entra"`), `entra.autoRedirect` →
/// `oauthAutoRedirect = "entra"`, and `entraGroups` → `oauthGroups`.
fn migrate_010_tinyauth_oauth_providers(doc: &mut DocumentMut) {
    move_dotted(
        doc,
        "services.tinyauth.entraGroups",
        "services.tinyauth.oauthGroups",
    );
    let Some(mut entra) = remove_dotted(doc, "services.tinyauth.entra") else {
        return;
    };
    let Some(table) = entra.as_table_like_mut() else {
        return;
    };
    let auto_redirect = table
        .remove("autoRedirect")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // `enabled` used to default to false; providers now default to enabled.
    if table.get("enabled").is_none() {
        table.insert("enabled", toml_edit::value(false));
    }
    table.insert("type", toml_edit::value("entra"));
    if auto_redirect {
        insert_dotted(
            doc,
            "services.tinyauth.oauthAutoRedirect",
            toml_edit::value("entra"),
        );
    }
    insert_dotted(doc, "services.tinyauth.oauth.entra", entra);
}

fn profile_has_key(doc: &DocumentMut, profile: &str, key: &str) -> bool {
    doc.get("neo-cli")
        .and_then(|t| t.get(profile))
        .and_then(|t| t.as_table())
        .map(|t| t.contains_key(key))
        .unwrap_or(false)
}

fn is_local_config_path(path: &str) -> bool {
    let p = path.trim();
    if p.is_empty() {
        return false;
    }
    if p.starts_with("./") || p.starts_with("../") {
        return true;
    }
    if !p.starts_with('/') {
        return true; // relative
    }
    // Absolute under a home directory is typically laptop scaffolding.
    p.starts_with("/home/") || p.starts_with("/Users/")
}

fn is_server_config_path(path: &str) -> bool {
    let p = path.trim();
    p.contains("/var/neo") || p.contains("AppData/configuration") || p.contains("/DATA/AppData/")
}

fn get_applied(doc: &DocumentMut) -> Vec<String> {
    doc.get("migrations")
        .and_then(|m| m.get("applied"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn set_applied(doc: &mut DocumentMut, applied: &[String]) {
    let tbl = doc
        .entry("migrations")
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .expect("migrations table");
    let mut arr = Array::new();
    for s in applied {
        arr.push(s.as_str());
    }
    tbl.insert("applied", Item::Value(toml_edit::Value::Array(arr)));
}

/// `(from, to)` dotted settings path; empty `to` deletes `from`.
type KeyRename = (&'static str, &'static str);

struct Migration {
    id: &'static str,
    renames: &'static [KeyRename],
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        id: "001-rename-legacy-neo-nixos-cli-and-core-keys",
        renames: &[
            ("cli", "neo-cli"),
            ("volumes", "core.volumes"),
            ("ssh", "core.ssh"),
            ("timeZone", "core.timeZone"),
            ("uid", "core.uid"),
            ("gid", "core.gid"),
            ("device.hostname", "core.hostname"),
            ("users.hashedPassword", "core.hashedLinuxPassword"),
            ("device", ""),
            ("users", ""),
        ],
    },
    Migration {
        id: "002-backup-ssh-connection-keys",
        renames: &[
            ("services.backup.remoteServer", "services.backup.host"),
            ("services.backup.remoteUser", "services.backup.user"),
            (
                "services.backup.sshExtraOptions",
                "services.backup.extraOptions",
            ),
        ],
    },
    // OpenClaw is unmaintained and removed from Neo. Drop the whole table so
    // secrets (gatewayToken, telegramBotToken, *ApiKey, etc.) leave settings.toml.
    Migration {
        id: "005-remove-openclaw",
        renames: &[("services.openclaw", "")],
    },
];

fn remove_dotted(doc: &mut DocumentMut, path: &str) -> Option<Item> {
    let parts: Vec<&str> = path.split('.').filter(|p| !p.is_empty()).collect();
    let (leaf, dirs) = parts.split_last()?;
    let mut cur = doc.as_table_mut();
    for p in dirs {
        cur = cur.get_mut(p)?.as_table_mut()?;
    }
    cur.remove(leaf)
}

/// Insert at a dotted path, creating tables on the way. A table value merges into an
/// existing table; a non-table on the way aborts.
fn insert_dotted(doc: &mut DocumentMut, path: &str, value: Item) {
    let parts: Vec<&str> = path.split('.').filter(|p| !p.is_empty()).collect();
    let Some((leaf, dirs)) = parts.split_last() else {
        return;
    };
    let mut cur = doc.as_table_mut();
    for p in dirs {
        cur = match cur
            .entry(p)
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
        {
            Some(t) => t,
            None => return,
        };
    }
    if let (Some(dst), Some(src)) = (
        cur.get_mut(leaf).and_then(Item::as_table_mut),
        value.as_table(),
    ) {
        for (k, v) in src.iter() {
            dst.insert(k, v.clone());
        }
        return;
    }
    cur.insert(leaf, value);
}

fn move_dotted(doc: &mut DocumentMut, from: &str, to: &str) -> bool {
    let Some(val) = remove_dotted(doc, from) else {
        return false;
    };
    if !to.is_empty() {
        insert_dotted(doc, to, val);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_010_moves_entra_into_oauth_providers() {
        let raw = r#"
[services.tinyauth]
enabled = true
entraGroups = ["firma"]

[services.tinyauth.entra]
enabled = true
autoRedirect = true
clientId = "cid"
tenantId = "tid"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let ta = doc["services"]["tinyauth"].as_table().expect("tinyauth");
        assert!(ta.get("entra").is_none());
        assert!(ta.get("entraGroups").is_none());
        assert_eq!(ta["oauthGroups"].as_array().map(|a| a.len()), Some(1));
        assert_eq!(ta["oauthAutoRedirect"].as_str(), Some("entra"));
        let p = &ta["oauth"]["entra"];
        assert_eq!(p["type"].as_str(), Some("entra"));
        assert_eq!(p["enabled"].as_bool(), Some(true));
        assert_eq!(p["clientId"].as_str(), Some("cid"));
        assert!(p.get("autoRedirect").is_none());
    }

    #[test]
    fn migration_010_keeps_disabled_entra_disabled() {
        let raw = r#"
[services.tinyauth.entra]
clientId = "cid"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let p = &doc["services"]["tinyauth"]["oauth"]["entra"];
        assert_eq!(p["enabled"].as_bool(), Some(false));
    }

    #[test]
    fn migration_003_drops_retired_service_sections() {
        let raw = r#"
[neo-service]
enabled = true
bootstrapEnabled = true
autoUpdateEnabled = false
autoUpdateTimer = "daily"
garbageCollectOlderThen = "7d"
plugins = ["github:example/plugin"]
neoInput = "github:example/neo"
configPath = "/var/neo/DATA/AppData/configuration"

[nixos]
enabled = true
repoUrl = "https://example.test/cfg.git"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));

        assert!(doc.get("neo-service").is_none());
        assert!(doc.get("nixos").is_none());

        let updater = doc
            .get("services")
            .and_then(|s| s.get("system-updater"))
            .and_then(|t| t.as_table())
            .expect("services.system-updater");
        assert_eq!(
            updater.get("enabled").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            updater.get("schedule").and_then(|v| v.as_str()),
            Some("daily")
        );
        assert_eq!(
            updater
                .get("garbageCollectOlderThan")
                .and_then(|v| v.as_str()),
            Some("7d")
        );
        assert!(updater.get("garbageCollectOlderThen").is_none());

        let plugins = doc
            .get("core")
            .and_then(|c| c.get("plugins"))
            .and_then(|p| p.as_array())
            .expect("core.plugins");
        assert_eq!(
            plugins
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>(),
            vec!["github:example/plugin"]
        );

        let cli = doc.get("neo-cli").and_then(|t| t.as_table()).unwrap();
        assert!(
            cli.get("neoInput").is_none(),
            "shared neoInput moves onto the profiles"
        );
        assert_eq!(
            cli.get("local")
                .and_then(|t| t.get("neoInput"))
                .and_then(|v| v.as_str()),
            Some("github:example/neo")
        );
        assert_eq!(
            cli.get("server")
                .and_then(|t| t.get("neoInput"))
                .and_then(|v| v.as_str()),
            Some("github:example/neo")
        );
        assert_eq!(
            cli.get("repoUrl").and_then(|v| v.as_str()),
            Some("https://example.test/cfg.git")
        );
        assert_eq!(
            cli.get("server")
                .and_then(|t| t.get("configPath"))
                .and_then(|v| v.as_str()),
            Some("/var/neo/DATA/AppData/configuration")
        );

        let out = doc.to_string();
        assert!(!out.contains("bootstrapEnabled"));
        assert!(!out.contains("autoUpdateEnabled"));
        assert!(
            !apply_migrations(&mut doc),
            "second run must report no further migrations"
        );
    }

    #[test]
    fn migration_007_keeps_a_local_checkout_off_the_server_profile() {
        let raw = r#"
[neo-cli]
neoInput = "git+file:/home/damo/Documents/projects/homeserver/neo"
template = "/home/damo/Documents/projects/homeserver/neo#homeserver"

[neo-cli.local]
configPath = "/home/damo/Documents/projects/homeserver/neo/build"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));

        let cli = doc.get("neo-cli").and_then(|t| t.as_table()).unwrap();
        assert!(cli.get("neoInput").is_none());
        assert!(cli.get("template").is_none());
        assert_eq!(
            cli.get("local")
                .and_then(|t| t.get("neoInput"))
                .and_then(|v| v.as_str()),
            Some("git+file:/home/damo/Documents/projects/homeserver/neo")
        );
        assert_eq!(
            cli.get("local")
                .and_then(|t| t.get("template"))
                .and_then(|v| v.as_str()),
            Some("/home/damo/Documents/projects/homeserver/neo#homeserver")
        );
        let server = cli.get("server").and_then(|t| t.as_table());
        assert!(
            server
                .map(|t| !t.contains_key("neoInput") && !t.contains_key("template"))
                .unwrap_or(true),
            "server profile must keep the GitHub default"
        );
        assert!(
            !apply_migrations(&mut doc),
            "second run must report no further migrations"
        );
    }

    #[test]
    fn migration_008_moves_git_identity_to_core() {
        let raw = r#"
[core.git]
userEmail = "kept@example.com"

[neo-cli]
gitUserName = "Ops"
gitUserEmail = "old@example.com"
defaultBranch = "master"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));

        let git = doc
            .get("core")
            .and_then(|t| t.get("git"))
            .and_then(|t| t.as_table())
            .unwrap();
        assert_eq!(git.get("userName").and_then(|v| v.as_str()), Some("Ops"));
        assert_eq!(
            git.get("userEmail").and_then(|v| v.as_str()),
            Some("kept@example.com"),
            "an existing core.git value wins"
        );
        let cli = doc.get("neo-cli").and_then(|t| t.as_table()).unwrap();
        assert!(!cli.contains_key("gitUserName"));
        assert!(!cli.contains_key("gitUserEmail"));
        assert!(cli.contains_key("defaultBranch"));
    }

    #[test]
    fn migration_005_removes_openclaw_secrets() {
        let raw = r#"
[services.hermes]
enabled = true
xaiApiKey = "keep-me"

[services.openclaw]
enabled = true
gatewayToken = "secret-gateway"
telegramBotToken = "secret-telegram"
anthropicApiKey = "secret-anthropic"
openaiApiKey = "secret-openai"
xaiApiKey = "secret-xai"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));

        let services = doc.get("services").and_then(|s| s.as_table()).unwrap();
        assert!(
            !services.contains_key("openclaw"),
            "services.openclaw must be removed"
        );
        assert!(
            services.contains_key("hermes"),
            "unrelated services must stay"
        );

        let applied = get_applied(&doc);
        assert!(
            applied.iter().any(|id| id == "005-remove-openclaw"),
            "005-remove-openclaw must be recorded"
        );

        let out = doc.to_string();
        for secret in [
            "secret-gateway",
            "secret-telegram",
            "secret-anthropic",
            "secret-openai",
            "secret-xai",
            "gatewayToken",
            "telegramBotToken",
            "anthropicApiKey",
            "openaiApiKey",
        ] {
            assert!(
                !out.contains(secret),
                "migrated settings must not contain {secret:?}"
            );
        }
        assert!(out.contains("keep-me"), "hermes secrets must remain");
    }

    #[test]
    fn migration_005_is_idempotent() {
        let raw = r#"
[services.openclaw]
xaiApiKey = "secret-xai"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        assert!(
            !apply_migrations(&mut doc),
            "second run must report no further migrations"
        );
        assert!(!doc.to_string().contains("secret-xai"));
        assert!(doc
            .get("services")
            .and_then(|s| s.as_table())
            .map(|t| !t.contains_key("openclaw"))
            .unwrap_or(true));
    }

    #[test]
    fn migration_006_unifies_hermes_llm_from_xai_key() {
        let raw = r#"
[services.hermes]
enabled = true
xaiApiKey = "secret-xai"
openaiApiKey = "secret-openai"
defaultModel = "grok-4"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));

        let llm = doc
            .get("services")
            .and_then(|s| s.get("hermes"))
            .and_then(|h| h.get("llm"))
            .and_then(|t| t.as_table())
            .expect("services.hermes.llm");
        assert_eq!(llm.get("provider").and_then(|v| v.as_str()), Some("xai"));
        assert_eq!(
            llm.get("apiKey").and_then(|v| v.as_str()),
            Some("secret-xai")
        );
        assert_eq!(llm.get("model").and_then(|v| v.as_str()), Some("grok-4"));

        let hermes = doc
            .get("services")
            .and_then(|s| s.get("hermes"))
            .and_then(|t| t.as_table())
            .unwrap();
        for gone in [
            "xaiApiKey",
            "openaiApiKey",
            "anthropicApiKey",
            "openrouterApiKey",
            "modelProvider",
            "defaultModel",
        ] {
            assert!(!hermes.contains_key(gone), "{gone} must be removed");
        }
        assert!(
            !doc.to_string().contains("secret-openai"),
            "non-selected vendor keys must be dropped"
        );
    }

    #[test]
    fn migration_006_honors_explicit_model_provider() {
        let raw = r#"
[services.hermes]
modelProvider = "xai-oauth"
defaultModel = "grok-build-latest"
xaiApiKey = "unused-when-oauth"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let llm = doc
            .get("services")
            .and_then(|s| s.get("hermes"))
            .and_then(|h| h.get("llm"))
            .and_then(|t| t.as_table())
            .unwrap();
        assert_eq!(
            llm.get("provider").and_then(|v| v.as_str()),
            Some("xai-oauth")
        );
        assert_eq!(
            llm.get("apiKey").and_then(|v| v.as_str()),
            Some("unused-when-oauth")
        );
        assert_eq!(
            llm.get("model").and_then(|v| v.as_str()),
            Some("grok-build-latest")
        );
    }

    #[test]
    fn migration_006_preserves_existing_llm_and_drops_legacy_keys() {
        let raw = r#"
[services.hermes]
xaiApiKey = "old-key"
[services.hermes.llm]
provider = "anthropic"
apiKey = "keep-me"
model = "claude-sonnet-4"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let llm = doc
            .get("services")
            .and_then(|s| s.get("hermes"))
            .and_then(|h| h.get("llm"))
            .and_then(|t| t.as_table())
            .unwrap();
        assert_eq!(
            llm.get("provider").and_then(|v| v.as_str()),
            Some("anthropic")
        );
        assert_eq!(llm.get("apiKey").and_then(|v| v.as_str()), Some("keep-me"));
        let out = doc.to_string();
        assert!(!out.contains("old-key"));
        assert!(!out.contains("xaiApiKey"));
    }

    #[test]
    fn migration_006_is_idempotent() {
        let raw = r#"
[services.hermes]
xaiApiKey = "secret-xai"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        assert!(
            !apply_migrations(&mut doc),
            "second run must report no further migrations"
        );
        let llm = doc
            .get("services")
            .and_then(|s| s.get("hermes"))
            .and_then(|h| h.get("llm"))
            .and_then(|t| t.as_table())
            .unwrap();
        assert_eq!(
            llm.get("apiKey").and_then(|v| v.as_str()),
            Some("secret-xai")
        );
    }

    #[test]
    fn migration_011_drops_bootstrap_method_everywhere() {
        let raw = r#"
[neo-cli]
bootstrapMethod = "clone"
repoUrl = "https://example.test/cfg.git"

[neo-cli.server]
bootstrapMethod = "template"

[neo-service]
bootstrapMethod = "clone"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let cli = doc.get("neo-cli").and_then(|t| t.as_table()).unwrap();
        assert!(cli.get("bootstrapMethod").is_none());
        assert!(cli
            .get("server")
            .and_then(|t| t.get("bootstrapMethod"))
            .is_none());
        assert_eq!(
            cli.get("repoUrl").and_then(|v| v.as_str()),
            Some("https://example.test/cfg.git")
        );
    }

    #[test]
    fn migration_009_renames_gc_older_then_to_than() {
        let raw = r#"
[services.system-updater]
enabled = true
garbageCollectOlderThen = "30d"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let updater = doc
            .get("services")
            .and_then(|s| s.get("system-updater"))
            .and_then(|t| t.as_table())
            .expect("services.system-updater");
        assert_eq!(
            updater
                .get("garbageCollectOlderThan")
                .and_then(|v| v.as_str()),
            Some("30d")
        );
        assert!(updater.get("garbageCollectOlderThen").is_none());
        assert!(
            !apply_migrations(&mut doc),
            "second run must report no further migrations"
        );
    }

    #[test]
    fn migration_009_keeps_existing_than_and_drops_then() {
        let raw = r#"
[services.system-updater]
garbageCollectOlderThan = "60d"
garbageCollectOlderThen = "7d"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let updater = doc
            .get("services")
            .and_then(|s| s.get("system-updater"))
            .and_then(|t| t.as_table())
            .expect("services.system-updater");
        assert_eq!(
            updater
                .get("garbageCollectOlderThan")
                .and_then(|v| v.as_str()),
            Some("60d")
        );
        assert!(updater.get("garbageCollectOlderThen").is_none());
    }
}
