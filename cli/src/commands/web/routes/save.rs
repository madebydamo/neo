use std::sync::Arc;

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{post, State};
use toml_edit::{DocumentMut, Item, Table};

use crate::commands::web::locks::{try_lock, Blocked};
use crate::commands::web::plugins::{plugins_from_doc, services_to_remove, strip_service_tables};
use crate::commands::web::settings::json_to_toml_value;
use crate::commands::web::settings::save::{
    apply_payload_to_table, finish_save, load_settings_doc, route_secrets,
};
use crate::commands::web::types::AppConfig;
use crate::utils::locks::{LockGuard, LockSpec, OpInfo};

/// Settings writes wait for no one but are refused while an activation / update
/// commits or rewrites the config repo (409, the form toasts the reason).
fn settings_lock() -> Result<LockGuard, Blocked> {
    try_lock(
        &LockSpec::system_shared(),
        OpInfo::new("settings", "Saving settings"),
    )
}
use crate::commands::web::util::{core_section_ok, service_name_ok, CORE_NESTED_SECTIONS};

/// Non-empty JSON object payload, if any.
fn payload_map(payload: &serde_json::Value) -> Option<&serde_json::Map<String, serde_json::Value>> {
    payload.as_object().filter(|m| !m.is_empty())
}

/// Table built from a payload (dotted keys nest); `None` when it ends up empty.
fn payload_table(payload: &serde_json::Map<String, serde_json::Value>) -> Option<Table> {
    let mut tbl = Table::new();
    apply_payload_to_table(&mut tbl, payload);
    (!tbl.is_empty()).then_some(tbl)
}

#[post("/save/<service>", data = "<payload>")]
pub async fn save_service(
    config: &State<Arc<AppConfig>>,
    service: &str,
    payload: Json<serde_json::Value>,
) -> Result<Status, Blocked> {
    if !service_name_ok(service) {
        eprintln!("web: refusing save for invalid service name {:?}", service);
        return Ok(Status::BadRequest);
    }
    let _lock = settings_lock()?;
    Ok(save_service_inner(config, service, &payload).await)
}

async fn save_service_inner(
    config: &AppConfig,
    service: &str,
    payload: &serde_json::Value,
) -> Status {
    let settings_path = &config.settings_path;
    let mut doc = match load_settings_doc(settings_path) {
        Ok(d) => d,
        Err(s) => return s,
    };

    let Some(services_table) = doc
        .entry("services")
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
    else {
        return Status::InternalServerError;
    };

    // Replace [services.<service>] entirely (clean slate for this service's overrides).
    services_table.remove(service);
    if let Some(tbl) = payload_map(payload).and_then(payload_table) {
        services_table.insert(service, Item::Table(tbl));
    }

    let scope = ["services".to_string(), service.to_string()];
    if let Err(s) = route_secrets(config, &mut doc, Some(&scope)).await {
        return s;
    }
    finish_save(settings_path, &mut doc, config)
}

#[post("/save-core/<section>", data = "<payload>")]
pub async fn save_core_section(
    config: &State<Arc<AppConfig>>,
    section: &str,
    payload: Json<serde_json::Value>,
) -> Result<Status, Blocked> {
    if !core_section_ok(section) {
        eprintln!("web: refusing save for unknown core section {:?}", section);
        return Ok(Status::BadRequest);
    }
    let _lock = settings_lock()?;
    Ok(save_core_section_inner(config, section, &payload).await)
}

async fn save_core_section_inner(
    config: &AppConfig,
    section: &str,
    payload: &serde_json::Value,
) -> Status {
    let settings_path = &config.settings_path;
    let mut doc = match load_settings_doc(settings_path) {
        Ok(d) => d,
        Err(s) => return s,
    };

    let old_plugins = plugins_from_doc(&doc);

    // Remove possible old top-level location (for renames/migrations).
    // For the aggregate "core" we merge deltas instead of replacing/removing.
    if section != "core" {
        doc.remove(section);
    }

    if CORE_NESTED_SECTIONS.contains(&section) {
        if let Err(s) = apply_core_nested_section(&mut doc, section, payload) {
            return s;
        }
    } else if let Some(tbl) = payload_map(payload).and_then(payload_table) {
        // Top-level sections: neo-cli, disko
        doc.insert(section, Item::Table(tbl));
    }

    let new_plugins = plugins_from_doc(&doc);
    if old_plugins != new_plugins {
        let owners = {
            let mut ev = config.evaluator.lock().await;
            match ev.extract_plugin_owners().await {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("web: refusing plugin list save; ownership extract failed: {e:#}");
                    return Status::InternalServerError;
                }
            }
        };
        let gone = services_to_remove(&owners, &old_plugins, &new_plugins);
        if !gone.is_empty() {
            eprintln!("web: removing plugin config tables: {}", gone.join(", "));
            strip_service_tables(&mut doc, &gone);
        }
    }

    // A nested core section is replaced whole; the aggregate merges deltas.
    let scope = ["core".to_string(), section.to_string()];
    let scope =
        (CORE_NESTED_SECTIONS.contains(&section) && section != "core").then_some(&scope[..]);
    if let Err(s) = route_secrets(config, &mut doc, scope).await {
        return s;
    }
    finish_save(settings_path, &mut doc, config)
}

/// Ensure `[core]` exists and return a mutable reference, or InternalServerError.
fn ensure_core_table(doc: &mut DocumentMut) -> Result<&mut Table, Status> {
    if !doc.get("core").is_some_and(|c| c.is_table()) {
        doc.insert("core", Item::Table(Table::new()));
    }
    doc.get_mut("core")
        .and_then(|c| c.as_table_mut())
        .ok_or(Status::InternalServerError)
}

fn apply_core_nested_section(
    doc: &mut DocumentMut,
    section: &str,
    payload: &serde_json::Value,
) -> Result<(), Status> {
    let core_table = ensure_core_table(doc)?;
    if section != "core" {
        core_table.remove(section);
    }
    let Some(payload_map) = payload_map(payload) else {
        // Empty payload: nested key cleared above; drop an empty [core].
        if core_table.is_empty() {
            doc.remove("core");
        }
        return Ok(());
    };

    // Scalars under core (timeZone, uid, …) arrive as a single-key payload named after the section.
    // Aggregate "core" and nested tables (ssh, volumes) use multi-key / table payloads.
    if payload_map.len() == 1 && payload_map.contains_key(section) {
        // A single-key payload named "core" is not a scalar: no-op.
        if section != "core" {
            if let Some(tval) = payload_map.get(section).and_then(json_to_toml_value) {
                core_table.insert(section, Item::Value(tval));
            }
        }
    } else if section == "core" {
        // Merge aggregate core deltas (scalars + dotted sub keys) into `[core]`.
        let mut tbl = Table::new();
        apply_payload_to_table(&mut tbl, payload_map);
        for (k, item) in tbl.iter() {
            core_table.insert(k, item.clone());
        }
    } else if let Some(tbl) = payload_table(payload_map) {
        // Nested table under `[core].<section>` (ssh, volumes).
        core_table.insert(section, Item::Table(tbl));
    }
    Ok(())
}
