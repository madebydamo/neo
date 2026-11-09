//! Load and merge settings.toml (baked defaults + operator file).

use anyhow::{Context, Result};
use std::path::Path;
use toml_edit::{DocumentMut, Item, Table};

/// Load baked default settings (if any) and overlay the user file at `path`.
pub fn load_or_default_settings(path: &Path) -> Result<DocumentMut> {
    let default_str = baked_default_settings()?;
    let mut doc = if !default_str.is_empty() {
        default_str.parse().context("parse default TOML")?
    } else {
        DocumentMut::new()
    };
    if path.exists() {
        let user_str = std::fs::read_to_string(path).context("read user settings.toml")?;
        let user_doc: DocumentMut = user_str.parse().context("parse user TOML")?;
        merge_into(&mut doc, &user_doc);
    }
    Ok(doc)
}

/// Nix installs point `DEFAULT_SETTINGS_PATH` at a store file so changing
/// those defaults does not rebuild the crate. `option_env!` remains the
/// fallback for a binary compiled with the toml inlined.
fn baked_default_settings() -> Result<String> {
    match std::env::var("DEFAULT_SETTINGS_PATH") {
        Ok(path) if !path.is_empty() => std::fs::read_to_string(&path)
            .with_context(|| format!("read baked default settings from {path}")),
        _ => Ok(option_env!("DEFAULT_SETTINGS_TOML")
            .unwrap_or("")
            .to_string()),
    }
}

/// The item at a dotted path, e.g. `&["core", "ssh", "authorizedKeys"]`.
pub fn item_at<'a>(doc: &'a DocumentMut, path: &[&str]) -> Option<&'a Item> {
    let mut item = doc.as_item();
    for p in path {
        item = item.get(p)?;
    }
    Some(item)
}

pub fn str_at<'a>(doc: &'a DocumentMut, path: &[&str]) -> Option<&'a str> {
    item_at(doc, path)?.as_str()
}

pub fn bool_at(doc: &DocumentMut, path: &[&str]) -> Option<bool> {
    item_at(doc, path)?.as_bool()
}

/// The table at `key`, created (or replacing a value that is no table) if needed.
pub fn ensure_table<'a>(parent: &'a mut Table, key: &str) -> Option<&'a mut Table> {
    let item = parent.entry(key).or_insert(Item::None);
    if !item.is_table() {
        *item = Item::Table(Table::new());
    }
    item.as_table_mut()
}

/// `[disko] enabled` (ZFS disk layout instead of the plain filesystem).
pub fn disko_enabled(doc: &DocumentMut) -> bool {
    bool_at(doc, &["disko", "enabled"]).unwrap_or(false)
}

/// Every entry of `core.ssh.authorizedKeys`.
pub fn authorized_keys(doc: &DocumentMut) -> Vec<&str> {
    item_at(doc, &["core", "ssh", "authorizedKeys"])
        .and_then(|k| k.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default()
}

/// The host key recorded in `core.secrets.hostKey`.
pub fn recorded_host_key(doc: &DocumentMut) -> Option<&str> {
    str_at(doc, &["core", "secrets", "hostKey"])
}

fn merge_into(base: &mut DocumentMut, overlay: &DocumentMut) {
    for (k, v) in overlay.iter() {
        match v {
            toml_edit::Item::Table(t) => {
                if let Some(b) = base.get_mut(k).and_then(|x| x.as_table_mut()) {
                    for (ik, iv) in t.iter() {
                        // Nested profile tables (local/server): merge keys, do not replace whole table.
                        if let (Some(bt), Some(ot)) =
                            (b.get_mut(ik).and_then(|x| x.as_table_mut()), iv.as_table())
                        {
                            for (nk, nv) in ot.iter() {
                                bt.insert(nk, nv.clone());
                            }
                        } else {
                            b.insert(ik, iv.clone());
                        }
                    }
                } else {
                    base.insert(k, toml_edit::Item::Table(t.clone()));
                }
            }
            _ => {
                base.insert(k, v.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigates_dotted_paths() {
        let doc: DocumentMut = "[a.b]\ns = \"x\"\nt = true\n".parse().unwrap();
        assert_eq!(str_at(&doc, &["a", "b", "s"]), Some("x"));
        assert_eq!(bool_at(&doc, &["a", "b", "t"]), Some(true));
        assert_eq!(str_at(&doc, &["a", "b", "t"]), None);
        assert!(item_at(&doc, &["a", "nope", "s"]).is_none());
    }

    #[test]
    fn ensure_table_creates_and_replaces() {
        let mut doc: DocumentMut = "a = 1\n[b]\nkeep = 2\n".parse().unwrap();
        ensure_table(doc.as_table_mut(), "a").unwrap();
        assert!(doc["a"].is_table());
        ensure_table(doc.as_table_mut(), "b").unwrap();
        assert_eq!(doc["b"]["keep"].as_integer(), Some(2));
        ensure_table(doc.as_table_mut(), "c").unwrap();
        assert!(doc["c"].is_table());
    }
}
