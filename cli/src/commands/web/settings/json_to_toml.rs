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
        other => json_to_toml_value(other).map(Item::Value),
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
