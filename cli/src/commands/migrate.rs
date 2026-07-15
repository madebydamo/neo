use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use toml_edit::{Array, DocumentMut, Item, Table};

use crate::commands::toml_sort::sort_document_alphabetically;

pub fn migrate(config_path: &str, source_settings: &PathBuf, dry_run: bool) -> Result<()> {
    let live = PathBuf::from(config_path).join("settings.toml");
    let target = if live.exists() {
        live
    } else if source_settings.exists() {
        source_settings.clone()
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

fn apply_migrations(doc: &mut DocumentMut) -> bool {
    let mut applied = get_applied(doc);
    let orig = applied.len();
    for m in MIGRATIONS {
        if applied.iter().any(|a| a == m.id) {
            continue;
        }
        println!("Applying migration: {}", m.id);
        apply_renames(doc, m.renames);
        applied.push(m.id.to_string());
    }
    // Custom migrations that need more than path renames.
    if !applied.iter().any(|a| a == "003-split-neo-service") {
        println!("Applying migration: 003-split-neo-service");
        migrate_003_split_neo_service(doc);
        applied.push("003-split-neo-service".to_string());
    }
    if !applied
        .iter()
        .any(|a| a == "004-neo-cli-local-server-profiles")
    {
        println!("Applying migration: 004-neo-cli-local-server-profiles");
        migrate_004_neo_cli_profiles(doc);
        applied.push("004-neo-cli-local-server-profiles".to_string());
    }
    let did_new = applied.len() > orig;
    if did_new {
        set_applied(doc, &applied);
    }
    did_new
}

/// Split legacy [neo-service] into core.plugins, services.system-updater, and neo-cli.
fn migrate_003_split_neo_service(doc: &mut DocumentMut) {
    let Some(svc_item) = remove_dotted(doc, "neo-service") else {
        return;
    };
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
    if let Some(v) = svc.get("garbageCollectOlderThen") {
        insert_dotted(
            doc,
            "services.system-updater.garbageCollectOlderThen",
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

struct KeyRename {
    from: &'static str,
    to: &'static str,
}

struct Migration {
    id: &'static str,
    renames: &'static [KeyRename],
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        id: "001-rename-legacy-neo-nixos-cli-and-core-keys",
        renames: &[
            KeyRename {
                from: "nixos",
                to: "neo-service",
            },
            KeyRename {
                from: "cli",
                to: "neo-cli",
            },
            KeyRename {
                from: "volumes",
                to: "core.volumes",
            },
            KeyRename {
                from: "ssh",
                to: "core.ssh",
            },
            KeyRename {
                from: "timeZone",
                to: "core.timeZone",
            },
            KeyRename {
                from: "uid",
                to: "core.uid",
            },
            KeyRename {
                from: "gid",
                to: "core.gid",
            },
            KeyRename {
                from: "device.hostname",
                to: "core.hostname",
            },
            KeyRename {
                from: "users.hashedPassword",
                to: "core.hashedLinuxPassword",
            },
            KeyRename {
                from: "device",
                to: "",
            },
            KeyRename {
                from: "users",
                to: "",
            },
        ],
    },
    Migration {
        id: "002-backup-ssh-connection-keys",
        renames: &[
            KeyRename {
                from: "services.backup.remoteServer",
                to: "services.backup.host",
            },
            KeyRename {
                from: "services.backup.remoteUser",
                to: "services.backup.user",
            },
            KeyRename {
                from: "services.backup.sshExtraOptions",
                to: "services.backup.extraOptions",
            },
        ],
    },
];

fn apply_renames(doc: &mut DocumentMut, renames: &[KeyRename]) {
    for r in renames {
        move_dotted(doc, r.from, r.to);
    }
}

fn remove_dotted(doc: &mut DocumentMut, path: &str) -> Option<Item> {
    let parts: Vec<&str> = path.split('.').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [] => None,
        [key] => doc.remove(key),
        [head, mid @ .., leaf] => {
            let mut cur = doc.get_mut(head)?.as_table_mut()?;
            for p in mid {
                cur = cur.get_mut(p)?.as_table_mut()?;
            }
            cur.remove(leaf)
        }
    }
}

fn insert_dotted(doc: &mut DocumentMut, path: &str, value: Item) {
    let parts: Vec<&str> = path.split('.').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [] => {}
        [key] => {
            if let Some(ex) = doc.get_mut(key) {
                if let (Some(d), Some(s)) = (ex.as_table_mut(), value.as_table()) {
                    for (kk, vv) in s.iter() {
                        d.insert(kk, vv.clone());
                    }
                    return;
                }
            }
            doc.insert(key, value);
        }
        [head, mid @ .., leaf] => {
            {
                let top = doc.entry(head).or_insert(Item::Table(Table::new()));
                if top.as_table_mut().is_none() {
                    return;
                }
            }
            let mut cur = match doc.get_mut(head).and_then(|i| i.as_table_mut()) {
                Some(t) => t,
                None => return,
            };
            for p in mid {
                {
                    let child = cur.entry(p).or_insert(Item::Table(Table::new()));
                    if child.as_table_mut().is_none() {
                        return;
                    }
                }
                cur = match cur.get_mut(p).and_then(|i| i.as_table_mut()) {
                    Some(t) => t,
                    None => return,
                };
            }
            if let Some(ex) = cur.get_mut(leaf) {
                if let (Some(d), Some(s)) = (ex.as_table_mut(), value.as_table()) {
                    for (kk, vv) in s.iter() {
                        d.insert(kk, vv.clone());
                    }
                    return;
                }
            }
            cur.insert(leaf, value);
        }
    }
}

fn move_dotted(doc: &mut DocumentMut, from: &str, to: &str) -> bool {
    if let Some(val) = remove_dotted(doc, from) {
        if to.is_empty() {
            return true;
        }
        insert_dotted(doc, to, val);
        return true;
    }
    false
}
