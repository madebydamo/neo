//! Secret settings of two config versions, reduced to comparison tokens for
//! the semantic settings diff (no secret value leaves this module).
//!
//! A version's secrets are secrets.yaml ∪ secrets.sops.yaml. Encryption
//! re-randomizes every ciphertext, so a changed sops file is decrypted on both
//! sides; when that fails, only added / removed keys can be told apart.
use anyhow::Result;
use toml_edit::{DocumentMut, Item, Value};

use crate::utils::secrets::{self, Node, Tree};

/// Token of a secret leaf equal on both sides (or present on one side only).
const SAME: &str = "=";
/// Token of a secret leaf whose value differs between the sides.
const CHANGED: &str = "≠";

/// Raw files of one version (`""` = absent).
#[derive(Debug, Clone, Copy, Default)]
pub struct SecretTexts<'a> {
    /// settings.toml: secrets still inline there take part in the comparison,
    /// so moving one into a secrets file is no change.
    pub settings: &'a str,
    pub plain: &'a str,
    pub sops: &'a str,
}

/// Secrets of both sides as token trees (settings path → [`SAME`] / [`CHANGED`])
/// for `diff_settings`, plus a caveat when values could not be compared.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecretPair {
    pub old: Tree,
    pub new: Tree,
    pub note: Option<String>,
}

fn merge_tree(dst: &mut Tree, src: Tree) {
    for (k, n) in src {
        match (dst.get_mut(&k), n) {
            (Some(Node::Map(d)), Node::Map(s)) => merge_tree(d, s),
            (_, n) => {
                dst.insert(k, n);
            }
        }
    }
}

/// Every leaf replaced by [`SAME`]: keys compare, values do not.
fn opaque(tree: &Tree) -> Tree {
    tree.iter()
        .map(|(k, n)| match n {
            Node::Leaf(_) => (k.clone(), Node::Leaf(SAME.to_string())),
            Node::Map(m) => (k.clone(), Node::Map(opaque(m))),
        })
        .collect()
}

/// Leaves of `tree` → [`CHANGED`] where `other` has a different value at the same path.
fn tokens(tree: &Tree, other: Option<&Tree>) -> Tree {
    tree.iter()
        .map(|(k, n)| {
            let o = other.and_then(|t| t.get(k));
            let node = match n {
                Node::Leaf(v) => {
                    let differs = matches!(o, Some(Node::Leaf(w)) if w != v);
                    Node::Leaf(if differs { CHANGED } else { SAME }.to_string())
                }
                Node::Map(m) => Node::Map(tokens(
                    m,
                    match o {
                        Some(Node::Map(om)) => Some(om),
                        _ => None,
                    },
                )),
            };
            (k.clone(), node)
        })
        .collect()
}

fn leaf_paths(tree: &Tree, prefix: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    for (k, n) in tree {
        prefix.push(k.clone());
        match n {
            Node::Leaf(_) => out.push(prefix.clone()),
            Node::Map(m) => leaf_paths(m, prefix, out),
        }
        prefix.pop();
    }
}

fn has_leaf(tree: &Tree, path: &[String]) -> bool {
    let Some((head, rest)) = path.split_first() else {
        return false;
    };
    match (tree.get(head), rest.is_empty()) {
        (Some(Node::Leaf(_)), true) => true,
        (Some(Node::Map(m)), false) => has_leaf(m, rest),
        _ => false,
    }
}

fn insert_leaf(tree: &mut Tree, path: &[String], v: String) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut cur = tree;
    for k in parents {
        let e = cur
            .entry(k.clone())
            .or_insert_with(|| Node::Map(Tree::new()));
        let Node::Map(m) = e else { return };
        cur = m;
    }
    cur.insert(last.clone(), Node::Leaf(v));
}

/// Scalar at `path` in a settings document (tables and inline tables).
fn inline_value(doc: &DocumentMut, path: &[String]) -> Option<String> {
    let mut item: &Item = doc.as_item();
    let mut value: Option<&Value> = None;
    for k in path {
        match value {
            Some(v) => value = v.as_inline_table()?.get(k),
            None => match item.as_table_like()?.get(k)? {
                Item::Value(v) => value = Some(v),
                i => item = i,
            },
        }
    }
    match value? {
        Value::String(s) => Some(s.value().clone()),
        Value::Integer(i) => Some(i.value().to_string()),
        Value::Float(f) => Some(f.value().to_string()),
        Value::Boolean(b) => Some(b.value().to_string()),
        _ => None,
    }
}

/// Add to each side the inline settings value of every secret path it lacks.
fn pull_inline(o: &mut Tree, n: &mut Tree, old_settings: &str, new_settings: &str) {
    let mut paths = Vec::new();
    leaf_paths(o, &mut Vec::new(), &mut paths);
    leaf_paths(n, &mut Vec::new(), &mut paths);
    let docs = (
        old_settings.parse::<DocumentMut>().ok(),
        new_settings.parse::<DocumentMut>().ok(),
    );
    for path in paths {
        for (tree, doc) in [(&mut *o, &docs.0), (&mut *n, &docs.1)] {
            if has_leaf(tree, &path) {
                continue;
            }
            if let Some(v) = doc.as_ref().and_then(|d| inline_value(d, &path)) {
                insert_leaf(tree, &path, v);
            }
        }
    }
}

fn parse(text: &str, what: &str, notes: &mut Vec<String>) -> Tree {
    secrets::parse(text).unwrap_or_else(|e| {
        notes.push(format!("Could not read {what}: {e:#}"));
        Tree::new()
    })
}

/// Compare the secrets of two versions; `decrypt` turns a secrets.sops.yaml
/// text into its plain tree (only called when that file differs).
pub fn secret_pair(
    old: SecretTexts,
    new: SecretTexts,
    decrypt: &dyn Fn(&str) -> Result<Tree>,
) -> SecretPair {
    let mut notes = Vec::new();
    let mut o = parse(old.plain, secrets::PLAIN_FILE, &mut notes);
    let mut n = parse(new.plain, secrets::PLAIN_FILE, &mut notes);
    let mut comparable = true;
    if old.sops == new.sops {
        // Unchanged ciphertext: identical leaves on both sides.
        let t = parse(old.sops, secrets::SOPS_FILE, &mut notes);
        merge_tree(&mut o, t.clone());
        merge_tree(&mut n, t);
    } else {
        let dec = |text: &str| -> Result<Tree> {
            if text.trim().is_empty() {
                Ok(Tree::new())
            } else {
                decrypt(text)
            }
        };
        match dec(old.sops).and_then(|a| Ok((a, dec(new.sops)?))) {
            Ok((a, b)) => {
                merge_tree(&mut o, a);
                merge_tree(&mut n, b);
            }
            Err(e) => {
                notes.push(format!(
                    "Encrypted secrets could not be decrypted to compare ({e:#}): only added and removed secrets are listed."
                ));
                comparable = false;
                merge_tree(&mut o, parse(old.sops, secrets::SOPS_FILE, &mut notes));
                merge_tree(&mut n, parse(new.sops, secrets::SOPS_FILE, &mut notes));
            }
        }
    }
    pull_inline(&mut o, &mut n, old.settings, new.settings);
    let (old, new) = if comparable {
        (tokens(&o, None), tokens(&n, Some(&o)))
    } else {
        (opaque(&o), opaque(&n))
    };
    SecretPair {
        old,
        new,
        note: (!notes.is_empty()).then(|| notes.join(" ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;

    const OLD_PLAIN: &str =
        "services:\n  a:\n    token: \"t1\"\n    same: \"s\"\n    gone: \"g\"\n";
    const NEW_PLAIN: &str =
        "services:\n  a:\n    token: \"t2\"\n    same: \"s\"\n    added: \"x\"\n";

    fn leaf<'a>(t: &'a Tree, k: &str) -> Option<&'a str> {
        let Some(Node::Map(s)) = t.get("services") else {
            return None;
        };
        let Some(Node::Map(a)) = s.get("a") else {
            return None;
        };
        match a.get(k) {
            Some(Node::Leaf(v)) => Some(v.as_str()),
            _ => None,
        }
    }

    fn no_decrypt(_: &str) -> Result<Tree> {
        bail!("no key")
    }

    #[test]
    fn plain_values_become_tokens() {
        let p = secret_pair(
            SecretTexts {
                plain: OLD_PLAIN,
                ..Default::default()
            },
            SecretTexts {
                plain: NEW_PLAIN,
                ..Default::default()
            },
            &no_decrypt,
        );
        assert_eq!(leaf(&p.old, "token"), Some(SAME));
        assert_eq!(leaf(&p.new, "token"), Some(CHANGED));
        assert_eq!(leaf(&p.new, "same"), Some(SAME));
        assert_eq!(leaf(&p.old, "gone"), Some(SAME));
        assert_eq!(leaf(&p.new, "gone"), None);
        assert_eq!(leaf(&p.new, "added"), Some(SAME));
        assert!(p.note.is_none());
        assert!(!format!("{p:?}").contains("t2"));
    }

    #[test]
    fn changed_sops_is_decrypted_and_plain_to_sops_move_is_no_change() {
        let decrypt = |text: &str| -> Result<Tree> {
            Ok(secrets::parse(&text.replace("ENC", "")).expect("parse"))
        };
        let p = secret_pair(
            SecretTexts {
                plain: OLD_PLAIN,
                ..Default::default()
            },
            SecretTexts {
                sops: "ENCservices:\n  a:\n    token: \"t1\"\n",
                ..Default::default()
            },
            &decrypt,
        );
        assert_eq!(leaf(&p.new, "token"), Some(SAME));
        assert!(p.note.is_none());
    }

    #[test]
    fn inline_secret_moved_to_file_is_no_change() {
        let p = secret_pair(
            SecretTexts {
                settings: "[services.a]\nusers = { admin = \"pw\" }\n",
                ..Default::default()
            },
            SecretTexts {
                plain: "services:\n  a:\n    users:\n      admin: \"pw\"\n",
                ..Default::default()
            },
            &no_decrypt,
        );
        let d = crate::commands::web::diff::diff_settings(
            "[services.a]\nusers = { admin = \"pw\" }\n",
            "",
            &p.old,
            &p.new,
        )
        .expect("diff");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn undecryptable_sops_compares_keys_only() {
        let old = "services:\n  a:\n    token: ENC[1]\n    gone: ENC[2]\n";
        let new = "services:\n  a:\n    token: ENC[3]\n    added: ENC[4]\n";
        let p = secret_pair(
            SecretTexts {
                sops: old,
                ..Default::default()
            },
            SecretTexts {
                sops: new,
                ..Default::default()
            },
            &no_decrypt,
        );
        assert_eq!(leaf(&p.new, "token"), Some(SAME));
        assert_eq!(leaf(&p.new, "added"), Some(SAME));
        assert_eq!(leaf(&p.new, "gone"), None);
        assert!(p.note.is_some());
    }
}
