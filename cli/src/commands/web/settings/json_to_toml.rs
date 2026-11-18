use toml_edit::{Item, Table, Value};

fn json_object_to_inline_table(obj: &serde_json::Map<String, serde_json::Value>) -> Option<Value> {
    let mut it = toml_edit::InlineTable::new();
    for (k, val) in obj {
        if let Some(tv) = json_to_toml_value(val) {
            it.insert(k, tv);
        }
    }
    Some(Value::InlineTable(it))
}

pub fn json_to_toml_value(v: &serde_json::Value) -> Option<Value> {
    match v {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(b) => Some(Value::from(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(Value::from(i))
            } else {
                n.as_f64().map(Value::from)
            }
        }
        serde_json::Value::String(s) => Some(Value::from(s.clone())),
        serde_json::Value::Array(arr) => {
            let mut tarr = toml_edit::Array::new();
            for item in arr {
                if let Some(tv) = json_to_toml_value(item) {
                    tarr.push(tv);
                }
            }
            Some(Value::from(tarr))
        }
        // Nested objects as values (e.g. array-of-submodule items, dotted inserts).
        serde_json::Value::Object(obj) => json_object_to_inline_table(obj),
    }
}

/// Inline arrays longer than this many characters are written one item per line.
const INLINE_ARRAY_MAX: usize = 60;

/// Lay out a setting's list one item per line when it is long or holds inline
/// tables, so adding or removing one item changes one line of settings.toml.
/// Only for a key's own value: nested arrays stay inline.
pub fn layout_value(mut v: Value) -> Value {
    if let Value::Array(a) = &mut v {
        let long = a.len() > 1
            && (a.iter().any(|x| x.is_inline_table())
                || a.to_string().trim().len() > INLINE_ARRAY_MAX);
        if long {
            for item in a.iter_mut() {
                item.decor_mut().set_prefix("\n  ");
                item.decor_mut().set_suffix("");
            }
            a.set_trailing_comma(true);
            a.set_trailing("\n");
        }
    }
    v
}

/// [`json_to_toml_value`] for a key's own value, laid out by [`layout_value`].
pub fn json_to_toml_field(v: &serde_json::Value) -> Option<Value> {
    json_to_toml_value(v).map(layout_value)
}

/// Convert JSON to a TOML item. Objects become nested tables; other values
/// reuse [`json_to_toml_value`].
pub fn json_to_toml_item(v: &serde_json::Value) -> Option<Item> {
    match v {
        serde_json::Value::Null => None,
        serde_json::Value::Object(obj) => {
            let mut ttable = Table::new();
            for (k, val) in obj {
                if let Some(ti) = json_to_toml_item(val) {
                    ttable.insert(k, ti);
                }
            }
            Some(Item::Table(ttable))
        }
        other => json_to_toml_field(other).map(Item::Value),
    }
}

/// A settings.toml item as JSON (`None` for an empty item).
pub fn toml_item_to_json(item: &Item) -> Option<serde_json::Value> {
    fn table<'a>(
        entries: impl Iterator<Item = (&'a str, Option<serde_json::Value>)>,
    ) -> serde_json::Value {
        serde_json::Value::Object(
            entries
                .filter_map(|(k, v)| v.map(|v| (k.to_string(), v)))
                .collect(),
        )
    }
    fn value(v: &Value) -> serde_json::Value {
        match v {
            Value::String(s) => s.value().clone().into(),
            Value::Integer(i) => (*i.value()).into(),
            Value::Float(f) => (*f.value()).into(),
            Value::Boolean(b) => (*b.value()).into(),
            Value::Datetime(d) => d.value().to_string().into(),
            Value::Array(a) => a.iter().map(value).collect::<Vec<_>>().into(),
            Value::InlineTable(t) => table(t.iter().map(|(k, v)| (k, Some(value(v))))),
        }
    }
    match item {
        Item::None => None,
        Item::Value(v) => Some(value(v)),
        Item::Table(t) => Some(table(t.iter().map(|(k, i)| (k, toml_item_to_json(i))))),
        Item::ArrayOfTables(a) => Some(
            a.iter()
                .map(|t| table(t.iter().map(|(k, i)| (k, toml_item_to_json(i)))))
                .collect::<Vec<_>>()
                .into(),
        ),
    }
}

pub fn insert_dotted(table: &mut Table, dotted_key: &str, value: Value) {
    let parts: Vec<&str> = dotted_key.split('.').collect();
    if parts.is_empty() {
        return;
    }
    let mut current = table;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            current.insert(part, Item::Value(value));
            return;
        }
        {
            let entry = current.entry(part).or_insert(Item::Table(Table::new()));
            if !entry.is_table() {
                *entry = Item::Table(Table::new());
            }
        }
        if let Some(t) = current.get_mut(part).and_then(|e| e.as_table_mut()) {
            current = t;
        } else {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn written(v: serde_json::Value) -> String {
        let mut doc = toml_edit::DocumentMut::new();
        doc.insert("k", json_to_toml_item(&v).expect("item"));
        doc.to_string()
    }

    #[test]
    fn long_lists_get_one_item_per_line() {
        assert_eq!(
            written(json!(["local", "web"])),
            "k = [\"local\", \"web\"]\n"
        );
        assert_eq!(
            written(json!(["tmr-dashboard", "tmr-simulator", "vbsg-kpi", "bielcrawler"])),
            "k = [\n  \"tmr-dashboard\",\n  \"tmr-simulator\",\n  \"vbsg-kpi\",\n  \"bielcrawler\",\n]\n"
        );
        assert_eq!(
            written(json!([{ "a": 1 }, { "a": 2 }])),
            "k = [\n  { a = 1 },\n  { a = 2 },\n]\n"
        );
        // The written text reads back as the same list.
        let doc: toml_edit::DocumentMut = written(json!(["x".repeat(40), "y".repeat(40)]))
            .parse()
            .expect("parse");
        assert_eq!(doc["k"].as_array().map(|a| a.len()), Some(2));
    }
}
