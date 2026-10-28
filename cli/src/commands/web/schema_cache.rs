//! Process-local cache of option schemas for helper script resolution.
//! Populated on pane load; helper runs use the cache on hit (no nix evaluator mutex).

use std::collections::HashMap;

use super::nix::value_to_display;
use super::types::{AppConfig, OptionPaneContext, OptionSchema};
use crate::utils::secrets::{self, Backend, Node, Tree, PLACEHOLDER_PREFIX};
use crate::utils::secrets_store;

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct SchemaCacheKey {
    pub is_core: bool,
    pub name: String,
}

#[derive(Default, Debug)]
pub struct SchemaCache {
    entries: HashMap<SchemaCacheKey, Vec<OptionSchema>>,
}

impl SchemaCache {
    pub fn get(&self, is_core: bool, name: &str) -> Option<Vec<OptionSchema>> {
        let key = SchemaCacheKey {
            is_core,
            name: name.to_string(),
        };
        self.entries.get(&key).cloned()
    }

    pub fn put(&mut self, is_core: bool, name: &str, options: Vec<OptionSchema>) {
        let key = SchemaCacheKey {
            is_core,
            name: name.to_string(),
        };
        self.entries.insert(key, options);
    }

    pub fn invalidate_all(&mut self) {
        self.entries.clear();
    }
}

/// Extract a service (or core section) pane from the flake. Not cached.
pub async fn extract_pane(config: &AppConfig, is_core: bool, name: &str) -> OptionPaneContext {
    let mut ev = config.evaluator.lock().await;
    if is_core {
        ev.extract_neo_section(name).await
    } else {
        ev.extract_service_options(name).await
    }
}

/// Value of an `@NEO_SECRET:<a/b/c>@` placeholder in the decrypted tree.
fn secret_value<'a>(tree: &'a Tree, s: &str) -> Option<&'a str> {
    let path = s.strip_prefix(PLACEHOLDER_PREFIX)?.strip_suffix('@')?;
    let mut node: Option<&Node> = None;
    let mut level = tree;
    for key in path.split('/') {
        node = level.get(key);
        match node {
            Some(Node::Map(m)) => level = m,
            Some(Node::Leaf(_)) => {}
            None => return None,
        }
    }
    match node {
        Some(Node::Leaf(v)) => Some(v),
        _ => None,
    }
}

fn has_placeholder(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::String(s) => secrets::is_placeholder(s),
        serde_json::Value::Array(a) => a.iter().any(has_placeholder),
        serde_json::Value::Object(m) => m.values().any(has_placeholder),
        _ => false,
    }
}

fn fill_placeholders(v: &mut serde_json::Value, tree: &Tree) {
    match v {
        serde_json::Value::String(s) => {
            if let Some(value) = secret_value(tree, s) {
                *s = value.to_string();
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(|x| fill_placeholders(x, tree)),
        serde_json::Value::Object(m) => m.values_mut().for_each(|x| fill_placeholders(x, tree)),
        _ => {}
    }
}

/// Encrypted configs evaluate secrets to placeholders: show the decrypted
/// values in the form, so editing looks the same as with plain settings.
/// Saving them unchanged re-encrypts nothing (Store::save_encrypted).
fn reveal_secrets(config: &AppConfig, pane: &mut OptionPaneContext) {
    if !pane
        .options
        .iter()
        .any(|o| o.current.as_ref().is_some_and(has_placeholder))
    {
        return;
    }
    let Backend::Sops(path) = secrets::backend_for(&config.settings_path) else {
        return;
    };
    let tree = match secrets_store::decrypt(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("web: showing secret placeholders, decrypt failed: {e:#}");
            return;
        }
    };
    for o in pane.options.iter_mut() {
        if let Some(current) = o.current.as_mut() {
            fill_placeholders(current, &tree);
            o.currentDisplay = value_to_display(current);
        }
    }
    if let Ok(json) = serde_json::to_string(&pane.options) {
        pane.options_json = json;
    }
}

/// Extract a pane for rendering and remember its options for helper runs.
pub async fn load_pane(config: &AppConfig, is_core: bool, name: &str) -> OptionPaneContext {
    let mut pane = extract_pane(config, is_core, name).await;
    reveal_secrets(config, &mut pane);
    config
        .schema_cache
        .write()
        .await
        .put(is_core, name, pane.options.clone());
    pane
}

/// Option schemas for helper resolution: cache hit, else extract (cached on success).
pub async fn load_options(
    config: &AppConfig,
    is_core: bool,
    name: &str,
) -> Result<Vec<OptionSchema>, String> {
    if let Some(opts) = config.schema_cache.read().await.get(is_core, name) {
        return Ok(opts);
    }
    let pane = extract_pane(config, is_core, name).await;
    if let Some(err) = pane.eval_error.error {
        return Err(err);
    }
    config
        .schema_cache
        .write()
        .await
        .put(is_core, name, pane.options.clone());
    Ok(pane.options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_filled_from_the_tree() {
        let tree = secrets::parse(
            "services:\n  rathole:\n    token: \"t0k\"\ncore:\n  git:\n    tokens:\n      github.com: \"pat\"\n",
        )
        .unwrap();
        let mut v = serde_json::json!({
            "token": "@NEO_SECRET:services/rathole/token@",
            "tokens": { "github.com": "@NEO_SECRET:core/git/tokens/github.com@" },
            "missing": "@NEO_SECRET:services/nope@",
            "plain": "x"
        });
        assert!(has_placeholder(&v));
        fill_placeholders(&mut v, &tree);
        assert_eq!(v["token"], "t0k");
        assert_eq!(v["tokens"]["github.com"], "pat");
        assert_eq!(v["missing"], "@NEO_SECRET:services/nope@");
        assert_eq!(v["plain"], "x");
    }
}
