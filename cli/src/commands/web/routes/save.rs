use std::fs;
use std::sync::Arc;

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{post, State};
use toml_edit::{DocumentMut, Item, Table};

use crate::commands::web::action_bar::broadcast_action_bar;
use crate::commands::web::settings::{insert_dotted, json_to_toml_item, json_to_toml_value};
use crate::commands::web::structs::AppConfig;

#[post("/save/<service>", data = "<payload>")]
pub fn save_service(
    config: &State<Arc<AppConfig>>,
    service: &str,
    payload: Json<serde_json::Value>,
) -> Status {
    // Guard against the client accidentally sending the literal fallback value
    // (or an empty service). This used to result in [services.service] in the TOML.
    if service.trim().is_empty() || service == "service" {
        eprintln!("web: refusing save for invalid service name {:?}", service);
        return Status::BadRequest;
    }

    let settings_path = &config.settings_path;

    // Ensure the target directory exists (e.g. first save after init into a fresh configPath).
    if let Some(parent) = settings_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let content = if settings_path.exists() {
        match fs::read_to_string(settings_path) {
            Ok(c) => c,
            Err(_) => return Status::InternalServerError,
        }
    } else {
        String::new()
    };

    let mut doc: DocumentMut = if content.trim().is_empty() {
        DocumentMut::new()
    } else {
        match content.parse() {
            Ok(d) => d,
            Err(_) => return Status::InternalServerError,
        }
    };

    // Ensure [services] table exists
    if !doc.contains_key("services") {
        doc.insert("services", Item::Table(Table::new()));
    }
    let services_table = match doc.get_mut("services").and_then(|s| s.as_table_mut()) {
        Some(t) => t,
        None => return Status::InternalServerError,
    };

    // Remove previous [services.<service>] entirely (clean slate for this service's overrides)
    services_table.remove(service);

    // If payload has no keys (or not an object), we just removed -> done
    let payload_map = match payload.as_object() {
        Some(m) if !m.is_empty() => m,
        _ => {
            // write back (may have removed the section)
            if let Err(_) = fs::write(settings_path, doc.to_string()) {
                return Status::InternalServerError;
            }
            broadcast_action_bar(&config);
            return Status::Ok;
        }
    };

    // Build new table for the service, handling dotted keys (e.g. "vpn.enabled", "foo.bar.baz")
    let mut svc_table = Table::new();
    for (k, v) in payload_map.iter() {
        if k.contains('.') {
            // Dotted names are always terminal leaf fields (scalar / list / etc.) in the UI schema
            if let Some(tval) = json_to_toml_value(v) {
                insert_dotted(&mut svc_table, k, tval);
            }
        } else if let Some(titem) = json_to_toml_item(v) {
            svc_table.insert(k, titem);
        }
    }

    if !svc_table.is_empty() {
        services_table.insert(service, Item::Table(svc_table));
    }

    if let Err(_) = fs::write(settings_path, doc.to_string()) {
        return Status::InternalServerError;
    }

    let ev = config.evaluator.clone();
    tokio::spawn(async move {
        let mut g = ev.lock().await;
        let _ = g.refresh().await;
    });
    broadcast_action_bar(&config);

    Status::Ok
}

#[post("/save-core/<section>", data = "<payload>")]
pub fn save_core_section(
    config: &State<Arc<AppConfig>>,
    section: &str,
    payload: Json<serde_json::Value>,
) -> Status {
    if section.trim().is_empty() {
        return Status::BadRequest;
    }
    let settings_path = &config.settings_path;
    if let Some(parent) = settings_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = if settings_path.exists() {
        match fs::read_to_string(settings_path) {
            Ok(c) => c,
            Err(_) => return Status::InternalServerError,
        }
    } else {
        String::new()
    };
    let mut doc: DocumentMut = if content.trim().is_empty() {
        DocumentMut::new()
    } else {
        match content.parse() {
            Ok(d) => d,
            Err(_) => return Status::InternalServerError,
        }
    };
    let core_sections = [
        "ssh",
        "volumes",
        "timeZone",
        "uid",
        "gid",
        "hostname",
        "hashedLinuxPassword",
        "core",
    ];
    let is_core = core_sections.contains(&section);
    // Remove possible old top-level location (for renames/migrations).
    // For the aggregate "core" we merge deltas instead of replacing/removing.
    if section != "core" {
        doc.remove(section);
    }
    if is_core {
        // Ensure [core] table
        if !doc.contains_key("core") || !doc.get("core").map_or(false, |c| c.is_table()) {
            doc.insert("core", Item::Table(Table::new()));
        }
        let core_table = doc.get_mut("core").unwrap().as_table_mut().unwrap();
        if section != "core" {
            core_table.remove(section);
        }
        let payload_map = match payload.as_object() {
            Some(m) if !m.is_empty() => m,
            _ => {
                if section == "core" {
                    // No deltas sent for aggregate core (e.g. all at defaults). Leave existing [core] intact.
                    // If we just ensured an empty one, clean it up.
                    if let Some(t) = doc.get("core").and_then(|c| c.as_table()) {
                        if t.is_empty() {
                            doc.remove("core");
                        }
                    }
                    if let Err(_) = fs::write(settings_path, doc.to_string()) {
                        return Status::InternalServerError;
                    }
                    let ev = config.evaluator.clone();
                    tokio::spawn(async move {
                        let mut g = ev.lock().await;
                        let _ = g.refresh().await;
                    });
                    broadcast_action_bar(&config);
                    return Status::Ok;
                }
                core_table.remove(section);
                if let Err(_) = fs::write(settings_path, doc.to_string()) {
                    return Status::InternalServerError;
                }
                let ev = config.evaluator.clone();
                tokio::spawn(async move {
                    let mut g = ev.lock().await;
                    let _ = g.refresh().await;
                });
                broadcast_action_bar(&config);
                return Status::Ok;
            }
        };
        // Scalars under core (timeZone, uid, gid, hostname, hashedLinuxPassword) are values under [core]
        // not bare at root. The extract renames the single field name to the section for them.
        // The aggregate "core" (for the cleaned grid) merges all its fields (scalars + dotted subs) without clearing siblings.
        if payload_map.len() == 1 && payload_map.contains_key(section) {
            if let Some(v) = payload_map.get(section) {
                if let Some(tval) = json_to_toml_value(v) {
                    if section != "core" {
                        core_table.insert(section, Item::Value(tval));
                    }
                }
            }
        } else {
            let mut tbl = Table::new();
            for (k, v) in payload_map.iter() {
                if k.contains('.') {
                    if let Some(tval) = json_to_toml_value(v) {
                        insert_dotted(&mut tbl, k, tval);
                    }
                } else if let Some(titem) = json_to_toml_item(v) {
                    tbl.insert(k, titem);
                }
            }
            if !tbl.is_empty() {
                if section == "core" {
                    // Merge deltas (scalars + dotted sub keys like "volumes.root", "ssh.authorizedKeys")
                    // directly into the core table. This supports editing all core options in one pane
                    // without losing unsent (default) siblings.
                    for (k, item) in tbl.iter() {
                        core_table.insert(k, item.clone());
                    }
                } else {
                    core_table.insert(section, Item::Table(tbl));
                }
            }
        }
    } else {
        // Top-level sections: neo-service, neo-cli, disko (and the aggregate "core" is handled in is_core branch)
        let payload_map = match payload.as_object() {
            Some(m) if !m.is_empty() => m,
            _ => {
                if let Err(_) = fs::write(settings_path, doc.to_string()) {
                    return Status::InternalServerError;
                }
                let ev = config.evaluator.clone();
                tokio::spawn(async move {
                    let mut g = ev.lock().await;
                    let _ = g.refresh().await;
                });
                broadcast_action_bar(&config);
                return Status::Ok;
            }
        };
        let mut tbl = Table::new();
        for (k, v) in payload_map.iter() {
            if k.contains('.') {
                if let Some(tval) = json_to_toml_value(v) {
                    insert_dotted(&mut tbl, k, tval);
                }
            } else if let Some(titem) = json_to_toml_item(v) {
                tbl.insert(k, titem);
            }
        }
        if !tbl.is_empty() {
            doc.insert(section, Item::Table(tbl));
        }
    }
    if let Err(_) = fs::write(settings_path, doc.to_string()) {
        return Status::InternalServerError;
    }

    let ev = config.evaluator.clone();
    tokio::spawn(async move {
        let mut g = ev.lock().await;
        let _ = g.refresh().await;
    });
    broadcast_action_bar(&config);

    Status::Ok
}
