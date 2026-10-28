//! Secret settings kept out of settings.toml (docs/design/secrets.md §3).
//!
//! `secrets.yaml` (plain) and `secrets.sops.yaml` (encrypted) sit next to
//! settings.toml and mirror its paths. Nix reads them with
//! `nix/lib/secrets-yaml.nix`; this module reads and writes the same subset:
//! nested maps, two-space indentation, quoted or plain scalars, `|`/`>` block
//! scalars. Writes are canonical (sorted keys, double-quoted values).
//!
//! Which settings are secret comes from `extract_secret_paths.nix`: a list of
//! paths where `*` matches any key of an attrsOf.

use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table, Value};

pub const PLAIN_FILE: &str = "secrets.yaml";
pub const SOPS_FILE: &str = "secrets.sops.yaml";
pub const PLACEHOLDER_PREFIX: &str = "@NEO_SECRET:";

const HEADER: &str = "# Secret settings (neo). Plain text: keep this repository private, or encrypt it with `neo secrets enable`.\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Leaf(String),
    Map(Tree),
}

pub type Tree = BTreeMap<String, Node>;

/// Where secret values live for the config repo that holds `settings_path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    /// Inline in settings.toml (no secrets file yet).
    Inline,
    /// secrets.yaml next to settings.toml.
    Plain(PathBuf),
    /// secrets.sops.yaml next to settings.toml.
    Sops(PathBuf),
}

pub fn backend_for(settings_path: &Path) -> Backend {
    let dir = settings_path.parent().unwrap_or(Path::new("."));
    let sops = dir.join(SOPS_FILE);
    let plain = dir.join(PLAIN_FILE);
    if sops.exists() {
        Backend::Sops(sops)
    } else if plain.exists() {
        Backend::Plain(plain)
    } else {
        Backend::Inline
    }
}

pub fn is_placeholder(s: &str) -> bool {
    s.starts_with(PLACEHOLDER_PREFIX)
}

// ---------------------------------------------------------------- parsing --

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn split_key(n: usize, body: &str) -> Result<(String, String)> {
    let (key, rest) = if let Some(stripped) = body.strip_prefix('"') {
        let end = stripped
            .find('"')
            .with_context(|| format!("line {n}: unterminated quoted key"))?;
        (stripped[..end].to_string(), &stripped[end + 1..])
    } else if let Some(stripped) = body.strip_prefix('\'') {
        let end = stripped
            .find('\'')
            .with_context(|| format!("line {n}: unterminated quoted key"))?;
        (stripped[..end].to_string(), &stripped[end + 1..])
    } else {
        // Bare key: up to the last ':' that ends the line or precedes a space.
        let mut cut = None;
        for (i, c) in body.char_indices() {
            if c == ' ' {
                break;
            }
            if c == ':' && (i + 1 == body.len() || body[i + 1..].starts_with(' ')) {
                cut = Some(i);
                break;
            }
        }
        let i = cut.with_context(|| {
            format!("line {n}: expected `key: value` or `key:` (lists, flow maps and complex keys are not supported)")
        })?;
        (body[..i].to_string(), &body[i..])
    };
    let rest = rest
        .strip_prefix(':')
        .with_context(|| format!("line {n}: expected `:` after key"))?;
    if !rest.is_empty() && !rest.starts_with(' ') {
        bail!("line {n}: expected a space after `:`");
    }
    Ok((key, rest.trim().to_string()))
}

fn unescape_double(n: usize, s: &str) -> Result<String> {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('0') => {}
            Some(o) => bail!("line {n}: unsupported escape \\{o}"),
            None => bail!("line {n}: dangling backslash"),
        }
    }
    Ok(out)
}

fn scalar(n: usize, t: &str) -> Result<String> {
    if let Some(inner) = t.strip_prefix('"') {
        // Closing quote = first unescaped `"`; only a comment may follow.
        let mut esc = false;
        for (i, c) in inner.char_indices() {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                let after = inner[i + 1..].trim();
                if !after.is_empty() && !after.starts_with('#') {
                    bail!("line {n}: text after quoted value");
                }
                return unescape_double(n, &inner[..i]);
            }
        }
        bail!("line {n}: unterminated double-quoted value");
    }
    if let Some(inner) = t.strip_prefix('\'') {
        let mut i = 0;
        let b = inner.as_bytes();
        while i < b.len() {
            if b[i] == b'\'' {
                if i + 1 < b.len() && b[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }
                let after = inner[i + 1..].trim();
                if !after.is_empty() && !after.starts_with('#') {
                    bail!("line {n}: text after quoted value");
                }
                return Ok(inner[..i].replace("''", "'"));
            }
            i += 1;
        }
        bail!("line {n}: unterminated single-quoted value");
    }
    if let Some(first) = t.chars().next() {
        if "{[&*!-|>%@`".contains(first) && t != "-" {
            bail!("line {n}: unsupported value `{t}` (quote it)");
        }
    }
    let plain = match t.find(" #") {
        Some(i) => &t[..i],
        None => t,
    };
    Ok(plain.trim().to_string())
}

fn block_value(style: char, chomp: &str, lines: &[String]) -> String {
    let body = if style == '|' {
        lines.join("\n")
    } else {
        let mut s = String::new();
        for (i, l) in lines.iter().enumerate() {
            if i == 0 {
                s.push_str(l);
            } else if l.is_empty() || lines[i - 1].is_empty() {
                s.push('\n');
                s.push_str(l);
            } else {
                s.push(' ');
                s.push_str(l);
            }
        }
        s
    };
    let trimmed = body.trim_end_matches('\n').to_string();
    match chomp {
        "-" => trimmed,
        "+" => body + "\n",
        _ => trimmed + "\n",
    }
}

struct Frame {
    indent: usize,
    key: String,
    child: Option<usize>,
}

fn insert(tree: &mut Tree, path: &[String], node: Node, n: usize) -> Result<()> {
    let (last, parents) = path.split_last().expect("non-empty path");
    let mut cur = tree;
    for k in parents {
        let entry = cur
            .entry(k.clone())
            .or_insert_with(|| Node::Map(Tree::new()));
        cur = match entry {
            Node::Map(m) => m,
            Node::Leaf(_) => bail!("line {n}: `{k}` is both a value and a map"),
        };
    }
    if cur.contains_key(last) {
        bail!("line {n}: duplicate key {}", path.join("."));
    }
    cur.insert(last.clone(), node);
    Ok(())
}

/// Parse the secrets YAML subset (the top-level `sops:` metadata is skipped).
pub fn parse(text: &str) -> Result<Tree> {
    let mut tree = Tree::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut skip = false;
    let mut open_maps: Vec<(usize, Vec<String>)> = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let n = i + 1;
        let line = lines[i];
        i += 1;
        let trimmed = line.trim_start_matches(' ');
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if line[..line.len() - trimmed.len()].contains('\t') || trimmed.starts_with('\t') {
            bail!("line {n}: tab in indentation");
        }
        let ind = indent_of(line);
        if ind == 0 && (trimmed == "---" || trimmed.starts_with("--- ")) {
            if tree.is_empty() && stack.is_empty() {
                continue;
            }
            bail!("line {n}: multiple YAML documents are not supported");
        }
        if skip && ind > 0 {
            continue;
        }
        let (key, rest) = split_key(n, trimmed)?;
        stack.retain(|f| f.indent < ind);
        match stack.last_mut() {
            None if ind != 0 => bail!("line {n}: bad indentation"),
            Some(p) if p.child.is_some_and(|c| c != ind) => bail!("line {n}: bad indentation"),
            Some(p) => p.child = Some(ind),
            None => {}
        }
        let mut path: Vec<String> = stack.iter().map(|f| f.key.clone()).collect();
        path.push(key.clone());
        if ind == 0 && key == "sops" {
            skip = true;
            stack.clear();
            continue;
        }
        skip = false;
        if rest.is_empty() || rest.starts_with('#') {
            open_maps.push((n, path));
            stack.push(Frame {
                indent: ind,
                key,
                child: None,
            });
            continue;
        }
        let head = rest.split(" #").next().unwrap_or("").trim();
        let mut hc = head.chars();
        if let Some(style @ ('|' | '>')) = hc.next() {
            let chomp: String = hc.collect();
            if !chomp.is_empty() && chomp != "-" && chomp != "+" {
                bail!("line {n}: unsupported block scalar header `{head}`");
            }
            let mut body: Vec<&str> = Vec::new();
            while i < lines.len() {
                let l = lines[i];
                if l.trim().is_empty() || indent_of(l) > ind {
                    body.push(l);
                    i += 1;
                } else {
                    break;
                }
            }
            let base = body
                .iter()
                .filter(|l| !l.trim().is_empty())
                .map(|l| indent_of(l))
                .min()
                .unwrap_or(0);
            let deindented: Vec<String> = body
                .iter()
                .map(|l| {
                    if l.trim().is_empty() {
                        String::new()
                    } else {
                        l[base..].to_string()
                    }
                })
                .collect();
            insert(
                &mut tree,
                &path,
                Node::Leaf(block_value(style, &chomp, &deindented)),
                n,
            )?;
            continue;
        }
        insert(&mut tree, &path, Node::Leaf(scalar(n, &rest)?), n)?;
    }
    for (n, path) in open_maps {
        if lookup(&tree, &path).is_none() {
            bail!(
                "line {n}: `{}:` has no value",
                path.last().cloned().unwrap_or_default()
            );
        }
    }
    Ok(tree)
}

fn lookup<'a>(tree: &'a Tree, path: &[String]) -> Option<&'a Node> {
    let (first, rest) = path.split_first()?;
    let node = tree.get(first)?;
    if rest.is_empty() {
        return Some(node);
    }
    match node {
        Node::Map(m) => lookup(m, rest),
        Node::Leaf(_) => None,
    }
}

// ---------------------------------------------------------------- writing --

fn bare_key_ok(k: &str) -> bool {
    let mut chars = k.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric() || "_.-".contains(c))
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.:-".contains(c))
        && !k.ends_with(':')
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn emit_tree(tree: &Tree, depth: usize, out: &mut String) {
    for (k, node) in tree {
        let key = if bare_key_ok(k) { k.clone() } else { quote(k) };
        out.push_str(&"  ".repeat(depth));
        match node {
            Node::Leaf(v) => {
                out.push_str(&key);
                out.push_str(": ");
                out.push_str(&quote(v));
                out.push('\n');
            }
            Node::Map(m) => {
                out.push_str(&key);
                out.push_str(":\n");
                emit_tree(m, depth + 1, out);
            }
        }
    }
}

/// Canonical text for secrets.yaml.
pub fn emit(tree: &Tree) -> String {
    let mut out = String::from(HEADER);
    emit_tree(&prune(tree.clone()), 0, &mut out);
    out
}

fn prune(tree: Tree) -> Tree {
    tree.into_iter()
        .filter_map(|(k, n)| match n {
            Node::Map(m) => {
                let m = prune(m);
                (!m.is_empty()).then_some((k, Node::Map(m)))
            }
            leaf => Some((k, leaf)),
        })
        .collect()
}

pub fn read_plain(path: &Path) -> Result<Tree> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    parse(&text).with_context(|| format!("parse {}", path.display()))
}

/// Write secrets.yaml atomically, owner-only.
pub fn write_plain(path: &Path, tree: &Tree) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension("yaml.tmp");
    {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .with_context(|| format!("write {}", tmp.display()))?;
        f.write_all(emit(tree).as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))
}

// -------------------------------------------------------- TOML ↔ secrets --

fn item_to_node(item: &Item) -> Option<Node> {
    match item {
        Item::Value(v) => value_to_node(v),
        Item::Table(t) => {
            let m: Tree = t
                .iter()
                .filter_map(|(k, i)| item_to_node(i).map(|n| (k.to_string(), n)))
                .collect();
            Some(Node::Map(m))
        }
        _ => None,
    }
}

fn value_to_node(v: &Value) -> Option<Node> {
    match v {
        Value::String(s) => Some(Node::Leaf(s.value().clone())),
        Value::Integer(i) => Some(Node::Leaf(i.value().to_string())),
        Value::Float(f) => Some(Node::Leaf(f.value().to_string())),
        Value::Boolean(b) => Some(Node::Leaf(b.value().to_string())),
        Value::InlineTable(t) => {
            let m: Tree = t
                .iter()
                .filter_map(|(k, v)| value_to_node(v).map(|n| (k.to_string(), n)))
                .collect();
            Some(Node::Map(m))
        }
        Value::Array(_) | Value::Datetime(_) => None,
    }
}

/// Every concrete settings path in `table` that matches `pattern` ("*" = any key).
fn matches(table: &Table, prefix: &[String], pattern: &[String], out: &mut Vec<Vec<String>>) {
    let Some((head, rest)) = pattern.split_first() else {
        return;
    };
    let keys: Vec<String> = if head == "*" {
        table.iter().map(|(k, _)| k.to_string()).collect()
    } else if table.contains_key(head) {
        vec![head.clone()]
    } else {
        vec![]
    };
    for k in keys {
        let mut path = prefix.to_vec();
        path.push(k.clone());
        if rest.is_empty() {
            out.push(path);
        } else if let Some(sub) = table.get(&k).and_then(as_table_like) {
            matches(&sub, &path, rest, out);
        }
    }
}

/// Inline tables are viewed as tables for matching.
fn as_table_like(item: &Item) -> Option<Table> {
    match item {
        Item::Table(t) => Some(t.clone()),
        Item::Value(Value::InlineTable(t)) => Some(t.clone().into_table()),
        _ => None,
    }
}

fn remove_path(table: &mut Table, path: &[String]) -> Option<Item> {
    let (head, rest) = path.split_first()?;
    if rest.is_empty() {
        return table.remove(head);
    }
    let removed = match table.get_mut(head)? {
        Item::Table(t) => remove_path(t, rest),
        Item::Value(Value::InlineTable(t)) => {
            let mut tt = t.clone().into_table();
            let r = remove_path(&mut tt, rest);
            *t = tt.into_inline_table();
            r
        }
        _ => None,
    };
    let empty = match table.get(head) {
        Some(Item::Table(t)) => t.is_empty(),
        Some(Item::Value(Value::InlineTable(t))) => t.is_empty(),
        _ => false,
    };
    if empty {
        table.remove(head);
    }
    removed
}

fn set_node(tree: &mut Tree, path: &[String], node: Node) {
    let (last, parents) = path.split_last().expect("non-empty path");
    let mut cur = tree;
    for k in parents {
        let entry = cur
            .entry(k.clone())
            .or_insert_with(|| Node::Map(Tree::new()));
        if matches!(entry, Node::Leaf(_)) {
            *entry = Node::Map(Tree::new());
        }
        let Node::Map(m) = entry else { unreachable!() };
        cur = m;
    }
    cur.insert(last.clone(), node);
}

fn remove_node(tree: &mut Tree, path: &[String]) {
    let Some((head, rest)) = path.split_first() else {
        return;
    };
    if rest.is_empty() {
        tree.remove(head);
        return;
    }
    if let Some(Node::Map(m)) = tree.get_mut(head) {
        remove_node(m, rest);
    }
}

fn contains_placeholder(node: &Node) -> bool {
    match node {
        Node::Leaf(s) => is_placeholder(s),
        Node::Map(m) => m.values().any(contains_placeholder),
    }
}

/// Outcome of moving secret settings out of a TOML document.
#[derive(Debug, Default)]
pub struct Split {
    /// Paths that now live in the secrets tree.
    pub moved: Vec<Vec<String>>,
    /// Paths dropped because the value is an encrypted placeholder (unchanged).
    pub kept_encrypted: Vec<Vec<String>>,
}

/// Move every secret setting of `doc` into `tree` (overwriting what was there).
/// Placeholders (an encrypted value echoed back by the form) are dropped from
/// the TOML and leave `tree` alone. `scope` limits the change to paths below it
/// (a service save), so a cleared secret is removed from the tree only there.
pub fn split_into(
    doc: &mut DocumentMut,
    tree: &mut Tree,
    patterns: &[Vec<String>],
    scope: Option<&[String]>,
) -> Split {
    let mut found = Vec::new();
    for p in patterns {
        matches(doc.as_table(), &[], p, &mut found);
    }
    found.sort();
    found.dedup();
    let mut out = Split::default();
    for path in &found {
        let Some(item) = remove_path(doc.as_table_mut(), path) else {
            continue;
        };
        let Some(node) = item_to_node(&item) else {
            continue;
        };
        if contains_placeholder(&node) {
            out.kept_encrypted.push(path.clone());
        } else {
            set_node(tree, path, node);
            out.moved.push(path.clone());
        }
    }
    // A secret cleared in the form (absent from the saved table) is removed.
    if let Some(scope) = scope {
        let mut stale = Vec::new();
        collect_paths(tree, &mut Vec::new(), &mut stale);
        for path in stale {
            let in_scope = path.starts_with(scope);
            let still = found.iter().any(|f| path.starts_with(f));
            let secret = patterns.iter().any(|p| pattern_covers(p, &path));
            if in_scope && secret && !still {
                remove_node(tree, &path);
            }
        }
    }
    out
}

fn collect_paths(tree: &Tree, prefix: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    for (k, n) in tree {
        prefix.push(k.clone());
        match n {
            Node::Leaf(_) => out.push(prefix.clone()),
            Node::Map(m) => collect_paths(m, prefix, out),
        }
        prefix.pop();
    }
}

/// A leaf path lies at or below a secret pattern.
fn pattern_covers(pattern: &[String], path: &[String]) -> bool {
    path.len() >= pattern.len()
        && pattern
            .iter()
            .zip(path)
            .all(|(p, k)| p == "*" || p == k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Vec<String> {
        s.split('.').map(String::from).collect()
    }

    #[test]
    fn parse_subset() {
        let t = parse(
            "# c\nservices:\n  dufs:\n    password: abc # note\n  r:\n    token: \"q\\\"x\\n\"\n    k: |\n      a\n      b\n\n    f: >-\n      x\n      y\ncore:\n  git:\n    tokens:\n      github.com: pat#x\n      git.example.com:2222: 'it''s'\nsops:\n  age:\n    - enc: |\n        zzz\n",
        )
        .unwrap();
        assert_eq!(
            lookup(&t, &p("services.dufs.password")),
            Some(&Node::Leaf("abc".into()))
        );
        assert_eq!(
            lookup(&t, &p("services.r.token")),
            Some(&Node::Leaf("q\"x\n".into()))
        );
        assert_eq!(
            lookup(&t, &p("services.r.k")),
            Some(&Node::Leaf("a\nb\n".into()))
        );
        assert_eq!(
            lookup(&t, &p("services.r.f")),
            Some(&Node::Leaf("x y".into()))
        );
        let tokens = match lookup(&t, &p("core.git.tokens")) {
            Some(Node::Map(m)) => m.clone(),
            _ => panic!(),
        };
        assert_eq!(tokens["github.com"], Node::Leaf("pat#x".into()));
        assert_eq!(tokens["git.example.com:2222"], Node::Leaf("it's".into()));
        assert!(!t.contains_key("sops"));
    }

    #[test]
    fn parse_rejects() {
        for bad in [
            "a:\n  - x\n",
            "a: {b: c}\n",
            "a:\n\tb: c\n",
            "a:\nb: c\n",
            "a: x\na: y\n",
            "a:\n    b: c\n  d: e\n",
            "  a: b\n",
            "a: &x v\n",
        ] {
            assert!(parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn emit_round_trips() {
        let mut t = Tree::new();
        set_node(&mut t, &p("services.dufs.password"), Node::Leaf("a\"b\\c\nd".into()));
        set_node(
            &mut t,
            &["core".into(), "git".into(), "tokens".into(), "git.example.com:2222".into()],
            Node::Leaf("x".into()),
        );
        set_node(&mut t, &p("services.odd"), Node::Leaf("#not comment".into()));
        let text = emit(&t);
        assert_eq!(parse(&text).unwrap(), t);
    }

    #[test]
    fn split_moves_secrets_and_keeps_placeholders() {
        let mut doc: DocumentMut = r#"
[services.rathole]
enabled = true
token = "tok"

[services.tinyauth.oauth.google]
clientId = "id"
clientSecret = "sec"

[services.vpn]
wireguardPrivateKey = "@NEO_SECRET:services/vpn/wireguardPrivateKey:ab@"

[core.git.tokens]
"github.com" = "pat"
"#
        .parse()
        .unwrap();
        let patterns = vec![
            p("services.rathole.token"),
            vec![
                "services".into(),
                "tinyauth".into(),
                "oauth".into(),
                "*".into(),
                "clientSecret".into(),
            ],
            p("services.vpn.wireguardPrivateKey"),
            p("core.git.tokens"),
        ];
        let mut tree = Tree::new();
        set_node(
            &mut tree,
            &p("services.vpn.wireguardPrivateKey"),
            Node::Leaf("real".into()),
        );
        let split = split_into(&mut doc, &mut tree, &patterns, None);
        assert_eq!(split.moved.len(), 3);
        assert_eq!(split.kept_encrypted, vec![p("services.vpn.wireguardPrivateKey")]);
        let s = doc.to_string();
        assert!(!s.contains("tok") && !s.contains("sec") && !s.contains("pat") && !s.contains("NEO_SECRET"));
        assert!(s.contains("clientId"));
        assert_eq!(
            lookup(&tree, &p("services.vpn.wireguardPrivateKey")),
            Some(&Node::Leaf("real".into()))
        );
        assert_eq!(
            lookup(&tree, &p("services.tinyauth.oauth.google.clientSecret")),
            Some(&Node::Leaf("sec".into()))
        );
    }

    #[test]
    fn split_scope_removes_cleared_secret() {
        let mut doc: DocumentMut = "[services.rathole]\nenabled = true\n".parse().unwrap();
        let mut tree = Tree::new();
        set_node(&mut tree, &p("services.rathole.token"), Node::Leaf("old".into()));
        set_node(&mut tree, &p("services.dufs.password"), Node::Leaf("keep".into()));
        let patterns = vec![p("services.rathole.token"), p("services.dufs.password")];
        let scope = p("services.rathole");
        split_into(&mut doc, &mut tree, &patterns, Some(&scope));
        assert_eq!(lookup(&tree, &p("services.rathole.token")), None);
        assert!(lookup(&tree, &p("services.dufs.password")).is_some());
    }
}
