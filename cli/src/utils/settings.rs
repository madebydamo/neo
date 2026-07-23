//! Load and merge settings.toml (baked defaults + operator file).

use anyhow::{Context, Result};
use std::path::PathBuf;
use toml_edit::DocumentMut;

/// Load baked default settings (if any) and overlay the user file at `path`.
pub fn load_or_default_settings(path: &PathBuf, _profile: &str) -> Result<DocumentMut> {
    let default_str = option_env!("DEFAULT_SETTINGS_TOML").unwrap_or("");
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
