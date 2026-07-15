use std::sync::Arc;

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{post, State};
use toml_edit::{DocumentMut, Item, Table};

use crate::commands::web::settings::json_to_toml_value;
use crate::commands::web::settings::save::{
    apply_payload_to_table, finish_save_state, load_settings_doc,
};
use crate::commands::web::structs::AppConfig;
use crate::commands::web::util::{core_section_ok, service_name_ok};

#[post("/save/<service>", data = "<payload>")]
pub fn save_service(
    config: &State<Arc<AppConfig>>,
    service: &str,
    payload: Json<serde_json::Value>,
) -> Status {
    if !service_name_ok(service) {
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
        _ => return finish_save_state(settings_path, &mut doc, config),
    };

    // Build new table for the service, handling dotted keys (e.g. "vpn.enabled", "foo.bar.baz")
    let mut svc_table = Table::new();
    apply_payload_to_table(&mut svc_table, payload_map);

    if !svc_table.is_empty() {
        services_table.insert(service, Item::Table(svc_table));
    }

    finish_save_state(settings_path, &mut doc, config)
}

/// Core sub-keys that live as tables/scalars under `[core]` (not top-level sections).
const CORE_NESTED_SECTIONS: &[&str] = &[
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

#[post("/save-core/<section>", data = "<payload>")]
pub fn save_core_section(
    config: &State<Arc<AppConfig>>,
    section: &str,
    payload: Json<serde_json::Value>,
) -> Status {
    if !core_section_ok(section) {
        eprintln!("web: refusing save for unknown core section {:?}", section);
        return Status::BadRequest;
    }

    let settings_path = &config.settings_path;
    let mut doc = match load_settings_doc(settings_path) {
        Ok(d) => d,
        Err(s) => return s,
    };

    let is_core_nested = CORE_NESTED_SECTIONS.contains(&section);

    // Remove possible old top-level location (for renames/migrations).
    // For the aggregate "core" we merge deltas instead of replacing/removing.
    if section != "core" {
        doc.remove(section);
    }

    let status = if is_core_nested {
        apply_core_nested_section(&mut doc, section, &payload)
    } else {
        // Top-level sections: neo-cli, disko
        save_toplevel_section(&mut doc, section, &payload)
    };

    match status {
        Ok(()) => finish_save_state(settings_path, &mut doc, config),
        Err(s) => s,
    }
}

/// Ensure `[core]` exists and return a mutable reference, or InternalServerError.
fn ensure_core_table(doc: &mut DocumentMut) -> Result<&mut Table, Status> {
    if !doc.contains_key("core") || !doc.get("core").map_or(false, |c| c.is_table()) {
        doc.insert("core", Item::Table(Table::new()));
    }
    doc.get_mut("core")
        .and_then(|c| c.as_table_mut())
        .ok_or(Status::InternalServerError)
}

fn drop_empty_core(doc: &mut DocumentMut) {
    if let Some(t) = doc.get("core").and_then(|c| c.as_table()) {
        if t.is_empty() {
            doc.remove("core");
        }
    }
}

fn apply_core_nested_section(
    doc: &mut DocumentMut,
    section: &str,
    payload: &serde_json::Value,
) -> Result<(), Status> {
    let payload_map = match payload.as_object() {
        Some(m) if !m.is_empty() => m,
        _ => {
            // Empty payload: clear nested key (when != "core"), drop empty [core].
            {
                let core_table = ensure_core_table(doc)?;
                if section != "core" {
                    core_table.remove(section);
                }
            }
            drop_empty_core(doc);
            return Ok(());
        }
    };

    let core_table = ensure_core_table(doc)?;
    if section != "core" {
        core_table.remove(section);
    }

    // Scalars under core (timeZone, uid, …) arrive as a single-key payload named after the section.
    // Aggregate "core" and nested tables (ssh, volumes) use multi-key / table payloads.
    if payload_map.len() == 1 && payload_map.contains_key(section) {
        if let Some(v) = payload_map.get(section) {
            save_core_scalar(core_table, section, v);
        }
    } else if section == "core" {
        save_core_aggregate(core_table, payload_map);
    } else {
        save_core_subtable(core_table, section, payload_map);
    }
    Ok(())
}

/// Insert a scalar value under `[core].<section>` (timeZone, uid, gid, hostname, hashedLinuxPassword).
fn save_core_scalar(core_table: &mut Table, section: &str, value: &serde_json::Value) {
    if section == "core" {
        // Single-key payload named "core" is not a scalar; treat as no-op (matches prior behavior).
        return;
    }
    if let Some(tval) = json_to_toml_value(value) {
        core_table.insert(section, Item::Value(tval));
    }
}

/// Merge aggregate core deltas (scalars + dotted sub keys) into `[core]`.
fn save_core_aggregate(
    core_table: &mut Table,
    payload_map: &serde_json::Map<String, serde_json::Value>,
) {
    let mut tbl = Table::new();
    apply_payload_to_table(&mut tbl, payload_map);
    for (k, item) in tbl.iter() {
        core_table.insert(k, item.clone());
    }
}

/// Insert a nested table under `[core].<section>` (ssh, volumes).
fn save_core_subtable(
    core_table: &mut Table,
    section: &str,
    payload_map: &serde_json::Map<String, serde_json::Value>,
) {
    let mut tbl = Table::new();
    apply_payload_to_table(&mut tbl, payload_map);
    if !tbl.is_empty() {
        core_table.insert(section, Item::Table(tbl));
    }
}

/// Top-level sections outside `[core]`: neo-cli, disko.
fn save_toplevel_section(
    doc: &mut DocumentMut,
    section: &str,
    payload: &serde_json::Value,
) -> Result<(), Status> {
    let payload_map = match payload.as_object() {
        Some(m) if !m.is_empty() => m,
        _ => return Ok(()),
    };
    let mut tbl = Table::new();
    apply_payload_to_table(&mut tbl, payload_map);
    if !tbl.is_empty() {
        doc.insert(section, Item::Table(tbl));
    }
    Ok(())
}
