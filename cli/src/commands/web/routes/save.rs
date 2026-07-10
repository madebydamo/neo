use std::sync::Arc;

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{post, State};
use toml_edit::{Item, Table};

use crate::commands::web::settings::json_to_toml_value;
use crate::commands::web::settings::save::{
    apply_payload_to_table, finish_save_state, load_settings_doc,
};
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
    let mut doc = match load_settings_doc(settings_path) {
        Ok(d) => d,
        Err(s) => return s,
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
        _ => return finish_save_state(settings_path, &doc, config),
    };

    // Build new table for the service, handling dotted keys (e.g. "vpn.enabled", "foo.bar.baz")
    let mut svc_table = Table::new();
    apply_payload_to_table(&mut svc_table, payload_map);

    if !svc_table.is_empty() {
        services_table.insert(service, Item::Table(svc_table));
    }

    finish_save_state(settings_path, &doc, config)
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
    let mut doc = match load_settings_doc(settings_path) {
        Ok(d) => d,
        Err(s) => return s,
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
                    return finish_save_state(settings_path, &doc, config);
                }
                core_table.remove(section);
                return finish_save_state(settings_path, &doc, config);
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
            apply_payload_to_table(&mut tbl, payload_map);
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
            _ => return finish_save_state(settings_path, &doc, config),
        };
        let mut tbl = Table::new();
        apply_payload_to_table(&mut tbl, payload_map);
        if !tbl.is_empty() {
            doc.insert(section, Item::Table(tbl));
        }
    }
    finish_save_state(settings_path, &doc, config)
}
