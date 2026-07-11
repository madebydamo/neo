use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use toml_edit::{Array, DocumentMut, Item, Table};

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
        fs::write(&target, doc.to_string()).context("write initial settings with migrations")?;
        println!("Initialized migrations marker in {}", target.display());
        return Ok(());
    }
    let content = fs::read_to_string(&target).context("read settings.toml for migration")?;
    let mut doc: DocumentMut = content.parse().context("parse settings.toml")?;
    let changed = apply_migrations(&mut doc);
    if changed {
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
    let did_new = applied.len() > orig;
    if did_new {
        set_applied(doc, &applied);
    }
    did_new
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
