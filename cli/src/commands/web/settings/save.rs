use std::fs;
use std::path::Path;
use std::sync::Arc;

use rocket::http::Status;
use toml_edit::{DocumentMut, Item, Table, Value};

use super::{insert_dotted, json_to_toml_item, json_to_toml_value};
use crate::commands::web::action_bar::broadcast_action_bar;
use crate::commands::web::structs::AppConfig;

pub fn load_settings_doc(path: &Path) -> Result<DocumentMut, Status> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = if path.exists() {
        fs::read_to_string(path).map_err(|_| Status::InternalServerError)?
    } else {
        String::new()
    };
    if content.trim().is_empty() {
        Ok(DocumentMut::new())
    } else {
        content.parse().map_err(|_| Status::InternalServerError)
    }
}

/// Sort all keys alphabetically (tables, subtables, inline tables) so saves produce
/// stable document order. Without this, remove+reinsert moves a service block to
/// the end of the file and diffs look like full rewrites.
pub fn sort_document_alphabetically(doc: &mut DocumentMut) {
    let mut next_pos = 0usize;
    sort_table_recursive(doc.as_table_mut(), &mut next_pos);
}

fn sort_table_recursive(table: &mut Table, next_pos: &mut usize) {
    table.sort_values();
    for (_, item) in table.iter_mut() {
        sort_item_inlines(item);
    }

    let keys: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();
    for key in keys {
        match table.get_mut(key.as_str()) {
            Some(Item::Table(t)) => {
                t.set_position(*next_pos);
                *next_pos += 1;
                sort_table_recursive(t, next_pos);
            }
            Some(Item::ArrayOfTables(aot)) => {
                for t in aot.iter_mut() {
                    t.set_position(*next_pos);
                    *next_pos += 1;
                    sort_table_recursive(t, next_pos);
                }
            }
            _ => {}
        }
    }
}

fn sort_item_inlines(item: &mut Item) {
    match item {
        Item::Value(v) => sort_value_inlines(v),
        Item::Table(t) => {
            for (_, child) in t.iter_mut() {
                sort_item_inlines(child);
            }
        }
        Item::ArrayOfTables(aot) => {
            for t in aot.iter_mut() {
                for (_, child) in t.iter_mut() {
                    sort_item_inlines(child);
                }
            }
        }
        Item::None => {}
    }
}

fn sort_value_inlines(v: &mut Value) {
    match v {
        Value::InlineTable(it) => {
            it.sort_values();
            for (_, val) in it.iter_mut() {
                sort_value_inlines(val);
            }
        }
        Value::Array(arr) => {
            for val in arr.iter_mut() {
                sort_value_inlines(val);
            }
        }
        _ => {}
    }
}

pub fn write_settings_doc(path: &Path, doc: &mut DocumentMut) -> Result<(), Status> {
    sort_document_alphabetically(doc);
    fs::write(path, doc.to_string()).map_err(|_| Status::InternalServerError)
}

/// Apply a JSON object payload into a TOML table (supports dotted leaf keys).
pub fn apply_payload_to_table(
    table: &mut Table,
    payload: &serde_json::Map<String, serde_json::Value>,
) {
    for (k, v) in payload.iter() {
        if k.contains('.') {
            if let Some(tval) = json_to_toml_value(v) {
                insert_dotted(table, k, tval);
            }
        } else if let Some(titem) = json_to_toml_item(v) {
            table.insert(k, titem);
        }
    }
}

/// Refresh evaluator, action bar, and schema cache after settings.toml changes
/// (save or restore). Safe to call from routes that rewrite settings without
/// going through [`finish_save`].
pub fn refresh_after_settings_change(config: &AppConfig) {
    let ev = config.evaluator.clone();
    tokio::spawn(async move {
        let mut g = ev.lock().await;
        let _ = g.refresh().await;
    });
    broadcast_action_bar(config);
    let cache = config.schema_cache.clone();
    tokio::spawn(async move {
        let mut c = cache.write().await;
        c.invalidate_all();
    });
}

/// After a successful settings write: refresh nix evaluator, action bar, schema cache.
pub fn after_save(config: &AppConfig) {
    refresh_after_settings_change(config);
}

pub fn finish_save(path: &Path, doc: &mut DocumentMut, config: &AppConfig) -> Status {
    if write_settings_doc(path, doc).is_err() {
        return Status::InternalServerError;
    }
    after_save(config);
    Status::Ok
}

/// Convenience when the caller holds `State<Arc<AppConfig>>`.
pub fn finish_save_state(path: &Path, doc: &mut DocumentMut, config: &Arc<AppConfig>) -> Status {
    finish_save(path, doc, config.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml_edit::value;

    #[test]
    fn sort_puts_services_and_keys_alphabetically() {
        let raw = r#"
[services.zeta]
port = 1
enabled = true

[services.alpha]
port = 2
enabled = false
nested = { z = 1, a = 2 }

[core]
hostname = "x"
uid = 1000
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();

        // Simulate save_service: remove + reinsert moves block without sort.
        {
            let services = doc["services"].as_table_mut().unwrap();
            services.remove("alpha");
            let mut t = Table::new();
            t.insert("port", value(2));
            t.insert("enabled", value(true));
            let mut it = toml_edit::InlineTable::new();
            it.insert("z", Value::from(1));
            it.insert("a", Value::from(2));
            t.insert("nested", Item::Value(Value::InlineTable(it)));
            services.insert("alpha", Item::Table(t));
        }

        sort_document_alphabetically(&mut doc);
        let out = doc.to_string();

        let core = out.find("[core]").expect("core section");
        let alpha = out.find("[services.alpha]").expect("alpha");
        let zeta = out.find("[services.zeta]").expect("zeta");
        assert!(
            core < alpha && alpha < zeta,
            "top-level and services sorted:\n{out}"
        );

        let alpha_block = &out[alpha..zeta];
        let enabled = alpha_block.find("enabled").unwrap();
        let nested = alpha_block.find("nested").unwrap();
        let port = alpha_block.find("port").unwrap();
        assert!(
            enabled < nested && nested < port,
            "keys within service sorted:\n{alpha_block}"
        );
        assert!(
            alpha_block.contains("nested = { a = 2, z = 1 }"),
            "inline table keys sorted:\n{alpha_block}"
        );
    }

    #[test]
    fn sort_places_new_service_in_alphabetical_slot() {
        let mut doc: DocumentMut = r#"
[services.zeta]
enabled = true

[services.alpha]
enabled = true
"#
        .parse()
        .unwrap();

        {
            let services = doc["services"].as_table_mut().unwrap();
            let mut t = Table::new();
            t.insert("enabled", value(true));
            services.insert("middle", Item::Table(t));
        }

        sort_document_alphabetically(&mut doc);
        let out = doc.to_string();
        let alpha = out.find("[services.alpha]").unwrap();
        let middle = out.find("[services.middle]").unwrap();
        let zeta = out.find("[services.zeta]").unwrap();
        assert!(
            alpha < middle && middle < zeta,
            "new service slotted in:\n{out}"
        );
    }
}
