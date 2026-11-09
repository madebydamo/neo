//! One write path for settings.toml: the settings form (`/save/…`), agent
//! proposals and (later) `neo settings set` all go through [`apply_edit`],
//! which takes the settings lock, edits the document, routes secrets and
//! finishes the save (sorted write, rekey, evaluator + action bar refresh).

use rocket::http::Status;
use toml_edit::{DocumentMut, Item, Table};

use super::json_to_toml_value;
use super::save::{apply_payload_to_table, finish_save, load_settings_doc, route_secrets};
use crate::commands::web::locks::{try_lock, Blocked};
use crate::commands::web::plugins::{services_to_remove, strip_service_tables};
use crate::commands::web::types::AppConfig;
use crate::commands::web::util::CORE_NESTED_SECTIONS;
use crate::utils::locks::{LockGuard, LockSpec, OpInfo};
use crate::utils::plugins::{
    plugins_are_tables, plugins_from_doc, validate_plugins, write_plugins,
};

/// A change to settings.toml.
pub enum Edit {
    /// The settings form: replace `[services.<service>]` with the payload.
    ReplaceService {
        service: String,
        payload: serde_json::Value,
    },
    /// The settings form for a core section (`core`, `ssh`, `neo-cli`, …).
    ReplaceCore {
        section: String,
        payload: serde_json::Value,
    },
    /// Agent proposals: set / remove single settings by path.
    Patch { ops: Vec<PatchOp> },
}

/// One resolved patch operation on a settings path (`["services","pihole","localIP"]`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatchOp {
    /// Write the value (a full TOML-able JSON value).
    Set {
        path: Vec<String>,
        value: serde_json::Value,
    },
    /// Remove the key so the option falls back to its default.
    Unset { path: Vec<String>, secret: bool },
}

/// Settings writes wait for no one but are refused while an activation / update
/// commits or rewrites the config repo (409, the form toasts the reason).
fn settings_lock() -> Result<LockGuard, Blocked> {
    try_lock(
        &LockSpec::system_shared(),
        OpInfo::new("settings", "Saving settings"),
    )
}

/// Apply `edit` to settings.toml under the settings lock.
pub async fn apply_edit(config: &AppConfig, edit: Edit) -> Result<Status, Blocked> {
    let _lock = settings_lock()?;
    Ok(match edit {
        Edit::ReplaceService { service, payload } => {
            save_service_inner(config, &service, &payload).await
        }
        Edit::ReplaceCore { section, payload } => {
            save_core_section_inner(config, &section, &payload).await
        }
        Edit::Patch { ops } => save_patch_inner(config, &ops).await,
    })
}

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
    finish_save(settings_path, &mut doc, config).await
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
    let old_plugins_item = doc.get("core").and_then(|c| c.get("plugins")).cloned();

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
    if old_plugins != new_plugins || !plugins_are_tables(&doc) {
        let errs = validate_plugins(&new_plugins);
        if !errs.is_empty() {
            eprintln!("web: refusing plugin save: {}", errs.join("; "));
            return Status::BadRequest;
        }
        // [core.plugins.<id>] tables; the form sends one object, an old config a list.
        // Rewrite onto the previous tables so unchanged entries keep their place.
        if let Some(core) = doc.get_mut("core").and_then(|c| c.as_table_mut()) {
            match old_plugins_item {
                Some(item) => core.insert("plugins", item),
                None => core.remove("plugins"),
            };
        }
        write_plugins(&mut doc, &new_plugins);
    }
    let old_ids: Vec<String> = old_plugins.iter().map(|p| p.id.clone()).collect();
    let new_ids: Vec<String> = new_plugins.iter().map(|p| p.id.clone()).collect();
    if old_ids.iter().any(|id| !new_ids.contains(id)) {
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
        let gone = services_to_remove(&owners, &old_ids, &new_ids);
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
    finish_save(settings_path, &mut doc, config).await
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

/// Table at `path` (created on the way), or None when a non-table is in the way.
fn table_at<'a>(doc: &'a mut DocumentMut, path: &[String]) -> Option<&'a mut Table> {
    let mut tbl = doc.as_table_mut();
    for seg in path {
        if !tbl
            .get(seg)
            .is_some_and(|i| i.is_table() || i.is_inline_table())
        {
            if tbl.contains_key(seg) {
                return None;
            }
            let mut t = Table::new();
            t.set_implicit(true);
            tbl.insert(seg, Item::Table(t));
        }
        let item = tbl.get_mut(seg)?;
        if item.is_inline_table() {
            let inline = item.as_inline_table()?.clone();
            *item = Item::Table(inline.into_table());
        }
        tbl = item.as_table_mut()?;
    }
    Some(tbl)
}

/// Remove `path` and drop tables it leaves empty.
fn remove_at(doc: &mut DocumentMut, path: &[String]) {
    fn rec(tbl: &mut Table, path: &[String]) {
        let Some((first, rest)) = path.split_first() else {
            return;
        };
        if rest.is_empty() {
            tbl.remove(first);
            return;
        }
        if let Some(child) = tbl.get_mut(first).and_then(|i| i.as_table_mut()) {
            rec(child, rest);
            if child.is_empty() {
                tbl.remove(first);
            }
        }
    }
    rec(doc.as_table_mut(), path);
}

/// Apply patch operations to an in-memory document (no secrets routing).
pub fn apply_ops(doc: &mut DocumentMut, ops: &[PatchOp]) -> Result<(), String> {
    for op in ops {
        match op {
            PatchOp::Set { path, value } => {
                let Some((key, parents)) = path.split_last() else {
                    return Err("empty settings path".into());
                };
                let Some(item) = super::json_to_toml_item(value) else {
                    return Err(format!(
                        "{}: value cannot be written to settings",
                        path.join(".")
                    ));
                };
                let Some(tbl) = table_at(doc, parents) else {
                    return Err(format!(
                        "{}: a parent setting is not a table",
                        path.join(".")
                    ));
                };
                tbl.insert(key, item);
            }
            PatchOp::Unset { path, .. } => remove_at(doc, path),
        }
    }
    Ok(())
}

async fn save_patch_inner(config: &AppConfig, ops: &[PatchOp]) -> Status {
    let settings_path = &config.settings_path;
    let mut doc = match load_settings_doc(settings_path) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if let Err(e) = apply_ops(&mut doc, ops) {
        eprintln!("web: refusing settings patch: {e}");
        return Status::UnprocessableEntity;
    }
    // New secret values move to the secrets file like a form save.
    if let Err(s) = route_secrets(config, &mut doc, None).await {
        return s;
    }
    // A cleared secret also leaves the secrets file (scope = that path).
    for op in ops {
        if let PatchOp::Unset { path, secret: true } = op {
            if let Err(s) = route_secrets(config, &mut doc, Some(path)).await {
                return s;
            }
        }
    }
    finish_save(settings_path, &mut doc, config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Vec<String> {
        s.split('.').map(str::to_string).collect()
    }

    #[test]
    fn set_creates_tables_and_unset_prunes_them() {
        let mut doc: DocumentMut = "[services.immich]\nenabled = true\n".parse().unwrap();
        apply_ops(
            &mut doc,
            &[
                PatchOp::Set {
                    path: p("services.pihole.enabled"),
                    value: serde_json::json!(true),
                },
                PatchOp::Set {
                    path: p("services.pihole.localIP"),
                    value: serde_json::json!("192.168.1.10"),
                },
            ],
        )
        .unwrap();
        assert_eq!(
            doc["services"]["pihole"]["localIP"].as_str(),
            Some("192.168.1.10")
        );
        assert_eq!(doc["services"]["immich"]["enabled"].as_bool(), Some(true));

        apply_ops(
            &mut doc,
            &[
                PatchOp::Unset {
                    path: p("services.pihole.enabled"),
                    secret: false,
                },
                PatchOp::Unset {
                    path: p("services.pihole.localIP"),
                    secret: false,
                },
            ],
        )
        .unwrap();
        assert!(doc["services"].get("pihole").is_none());
    }

    #[test]
    fn nested_entry_fields_and_their_secrets() {
        use crate::utils::secrets::{self, Node};
        let mut doc: DocumentMut = "[services.x.accounts.tinyauth]\nusername = \"admin\"\n"
            .parse()
            .unwrap();
        let mut tree = secrets::parse(
            "services:\n  x:\n    accounts:\n      tinyauth:\n        password: \"old\"\n",
        )
        .unwrap();
        let patterns = vec![p("services.x.accounts.*.password")];
        // A new entry, field by field (agent proposal: set username, ask password).
        apply_ops(
            &mut doc,
            &[
                PatchOp::Set {
                    path: p("services.x.accounts.tmr.username"),
                    value: serde_json::json!("tmr"),
                },
                PatchOp::Set {
                    path: p("services.x.accounts.tmr.password"),
                    value: serde_json::json!("typed"),
                },
            ],
        )
        .unwrap();
        secrets::split_into(&mut doc, &mut tree, &patterns, None);
        let s = doc.to_string();
        assert!(s.contains("tmr") && !s.contains("typed"), "{s}");
        assert_eq!(
            doc["services"]["x"]["accounts"]["tinyauth"]["username"].as_str(),
            Some("admin")
        );
        let accounts = |t: &secrets::Tree| match t.get("services").and_then(|n| match n {
            Node::Map(m) => m.get("x"),
            _ => None,
        }) {
            Some(Node::Map(x)) => match x.get("accounts") {
                Some(Node::Map(a)) => a.clone(),
                _ => Default::default(),
            },
            _ => Default::default(),
        };
        let a = accounts(&tree);
        assert!(
            matches!(a.get("tmr"), Some(Node::Map(m)) if m.get("password") == Some(&Node::Leaf("typed".into())))
        );
        assert!(a.contains_key("tinyauth"));

        // Removing an entry with a secret field clears its secret too (scope = entry).
        let entry = p("services.x.accounts.tinyauth");
        apply_ops(
            &mut doc,
            &[PatchOp::Unset {
                path: entry.clone(),
                secret: true,
            }],
        )
        .unwrap();
        secrets::split_into(&mut doc, &mut tree, &patterns, Some(&entry));
        // Emptied maps are pruned when the secrets file is written.
        let yaml = secrets::emit(&tree);
        assert!(!yaml.contains("tinyauth") && yaml.contains("tmr"), "{yaml}");
    }

    #[test]
    fn set_refuses_scalar_parent() {
        let mut doc: DocumentMut = "[services]\nfoo = 1\n".parse().unwrap();
        let err = apply_ops(
            &mut doc,
            &[PatchOp::Set {
                path: p("services.foo.bar"),
                value: serde_json::json!(1),
            }],
        );
        assert!(err.is_err());
    }
}
