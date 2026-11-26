//! `neo migrate`: settings.toml migrations. The registry ([`MIGRATIONS`]) is
//! the one ordered list; each custom migration lives in its own `mNNN_*.rs`
//! with its tests, renames stay inline in the registry.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{Array, DocumentMut, Item, Table};

use crate::utils::settings;

mod m003_split_neo_service;
mod m004_neo_cli_profiles;
mod m006_hermes_unified_llm;
mod m007_neo_cli_profile_inputs;
mod m008_core_git_identity;
mod m009_system_updater_gc_older_than;
mod m010_tinyauth_oauth_providers;
mod m011_drop_bootstrap_method;
mod m012_neo_two_themes;
mod m013_backup_keeps_ssh_key;
mod m014_plugin_tables;
mod m015_backup_destinations;
mod m016_nix_builders;

pub fn migrate(config_path: &str, source_settings: &Path, dry_run: bool) -> Result<()> {
    let live = PathBuf::from(config_path).join(settings::FILE);
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
        settings::save(&target, &mut doc).context("write initial settings with migrations")?;
        println!("Initialized migrations marker in {}", target.display());
        return Ok(());
    }
    let mut doc = settings::load(&target)?;
    check_migrations_table(&doc)?;
    let changed = apply_migrations(&mut doc);
    if changed {
        // Same ordering as web saves so update→migrate does not unsort the file.
        settings::save(&target, &mut doc).context("write migrated settings.toml")?;
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
    for &(id, ref step) in MIGRATIONS {
        if applied.iter().any(|a| a == id) {
            continue;
        }
        println!("Applying migration: {id}");
        match step {
            Step::Renames(renames) => {
                for &(from, to) in *renames {
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

/// Every migration, in the order it runs (sorted by id). Applied ids are kept
/// in `[migrations] applied`, so reordering never re-runs one that ran.
const MIGRATIONS: &[(&str, Step)] = &[
    (
        "001-rename-legacy-neo-nixos-cli-and-core-keys",
        Step::Renames(&[
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
        ]),
    ),
    (
        "002-backup-ssh-connection-keys",
        Step::Renames(&[
            ("services.backup.remoteServer", "services.backup.host"),
            ("services.backup.remoteUser", "services.backup.user"),
            (
                "services.backup.sshExtraOptions",
                "services.backup.extraOptions",
            ),
        ]),
    ),
    (
        "003-split-neo-service",
        Step::Custom(m003_split_neo_service::migrate),
    ),
    (
        "004-neo-cli-local-server-profiles",
        Step::Custom(m004_neo_cli_profiles::migrate),
    ),
    // OpenClaw is unmaintained and removed from Neo. Drop the whole table so
    // secrets (gatewayToken, telegramBotToken, *ApiKey, etc.) leave settings.toml.
    (
        "005-remove-openclaw",
        Step::Renames(&[("services.openclaw", "")]),
    ),
    (
        "006-hermes-unified-llm",
        Step::Custom(m006_hermes_unified_llm::migrate),
    ),
    (
        "007-neo-cli-profile-inputs",
        Step::Custom(m007_neo_cli_profile_inputs::migrate),
    ),
    (
        "008-core-git-identity",
        Step::Custom(m008_core_git_identity::migrate),
    ),
    (
        "009-system-updater-gc-older-than",
        Step::Custom(m009_system_updater_gc_older_than::migrate),
    ),
    (
        "010-tinyauth-oauth-providers",
        Step::Custom(m010_tinyauth_oauth_providers::migrate),
    ),
    // After 003/004, which may carry the key over from retired tables.
    (
        "011-drop-bootstrap-method",
        Step::Custom(m011_drop_bootstrap_method::migrate),
    ),
    (
        "012-neo-two-themes",
        Step::Custom(m012_neo_two_themes::migrate),
    ),
    (
        "013-backup-keeps-ssh-key",
        Step::Custom(m013_backup_keeps_ssh_key::migrate),
    ),
    (
        "014-plugin-tables",
        Step::Custom(m014_plugin_tables::migrate),
    ),
    // After 013, which pins sshKey at the top of services.backup.
    (
        "015-backup-destinations",
        Step::Custom(m015_backup_destinations::migrate),
    ),
    ("016-nix-builders", Step::Custom(m016_nix_builders::migrate)),
];

fn profile_has_key(doc: &DocumentMut, profile: &str, key: &str) -> bool {
    doc.get("neo-cli")
        .and_then(|t| t.get(profile))
        .and_then(|t| t.as_table())
        .map(|t| t.contains_key(key))
        .unwrap_or(false)
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

/// A `migrations` key neo cannot write to would otherwise lose the record of
/// what ran, so every migration runs again next time.
fn check_migrations_table(doc: &DocumentMut) -> Result<()> {
    match doc.get("migrations") {
        Some(m) if !m.is_table() => {
            anyhow::bail!("settings.toml: `migrations` must be a [migrations] table")
        }
        _ => Ok(()),
    }
}

fn set_applied(doc: &mut DocumentMut, applied: &[String]) {
    let entry = doc.entry("migrations").or_insert(Item::Table(Table::new()));
    let Some(tbl) = entry.as_table_mut() else {
        return;
    };
    let mut arr = Array::new();
    for s in applied {
        arr.push(s.as_str());
    }
    tbl.insert("applied", Item::Value(toml_edit::Value::Array(arr)));
}

/// `(from, to)` dotted settings path; empty `to` deletes `from`.
type KeyRename = (&'static str, &'static str);

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
    fn registry_is_sorted_by_unique_id() {
        let ids: Vec<&str> = MIGRATIONS.iter().map(|(id, _)| *id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            ids, sorted,
            "MIGRATIONS must be sorted by id, without duplicates"
        );
    }

    #[test]
    fn migrations_run_in_id_order() {
        let mut doc = DocumentMut::new();
        assert!(apply_migrations(&mut doc));
        let expected: Vec<&str> = MIGRATIONS.iter().map(|(id, _)| *id).collect();
        assert_eq!(get_applied(&doc), expected);
    }

    #[test]
    fn already_applied_ids_never_run_again() {
        // A config migrated under the old order (renames first) records 005
        // before 003; nothing may run again.
        let mut doc = DocumentMut::new();
        let mut old_order: Vec<String> = MIGRATIONS.iter().map(|(id, _)| id.to_string()).collect();
        let five = old_order.remove(4);
        old_order.insert(2, five);
        set_applied(&mut doc, &old_order);
        doc["services"] = toml_edit::table();
        doc["services"]["openclaw"] = toml_edit::table();
        assert!(!apply_migrations(&mut doc));
        assert!(doc["services"].get("openclaw").is_some());
        assert_eq!(get_applied(&doc), old_order);
    }

    #[test]
    fn malformed_migrations_key_is_an_error() {
        for bad in ["migrations = \"x\"\n", "migrations = { applied = [] }\n"] {
            let doc: DocumentMut = bad.parse().unwrap();
            assert!(check_migrations_table(&doc).is_err(), "{bad}");
        }
        let ok: DocumentMut = "[migrations]\napplied = []\n".parse().unwrap();
        assert!(check_migrations_table(&ok).is_ok());
        assert!(check_migrations_table(&DocumentMut::new()).is_ok());
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
}
