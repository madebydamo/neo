//! Stable alphabetical ordering for settings.toml documents.
//!
//! Web saves, migrations, and other writers must share this so the file does not
//! flip between hand/insertion order and sorted order across operations.

use toml_edit::{DocumentMut, Item, Table, Value};

/// Sort all keys alphabetically (tables, subtables, inline tables) so rewrites
/// produce stable document order. Without this, remove+reinsert (saves,
/// migrations) moves blocks around and diffs look like full rewrites.
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
