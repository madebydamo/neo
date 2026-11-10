//! Option panes from the evaluation pool (nix/pool.rs caches them per evaluation).

use super::nix::value_to_display;
use super::types::{AppConfig, OptionPaneContext, OptionSchema};
use crate::utils::secrets::{self, Backend, Node, Tree, PLACEHOLDER_PREFIX};
use crate::utils::secrets_store;

/// A service (or core section) pane from the serving evaluation, with settings
/// saved since laid over it. Secrets stay placeholders on encrypted configs.
pub async fn extract_pane(config: &AppConfig, is_core: bool, name: &str) -> OptionPaneContext {
    config.eval.pane(is_core, name).await
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

pub(crate) fn has_placeholder(v: &serde_json::Value) -> bool {
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

/// A pane for rendering, secrets revealed for the form.
pub async fn load_pane(config: &AppConfig, is_core: bool, name: &str) -> OptionPaneContext {
    let mut pane = extract_pane(config, is_core, name).await;
    reveal_secrets(config, &mut pane);
    pane
}

/// A pane's option schemas and values (secrets revealed) for helpers,
/// proposals and the assistant's tools. Err with the evaluation error.
pub async fn load_options(
    config: &AppConfig,
    is_core: bool,
    name: &str,
) -> Result<Vec<OptionSchema>, String> {
    let pane = load_pane(config, is_core, name).await;
    match pane.eval_error.error {
        Some(err) => Err(err),
        None => Ok(pane.options),
    }
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
