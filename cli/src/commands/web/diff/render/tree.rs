//! The file tree beside the diffs.
use std::collections::BTreeMap;

use super::super::model::{ChangeSet, FileChange};
use super::{counts_html, file_id, status_badge, ICON_CHEVRON, ICON_FOLDER};
use crate::commands::web::util::{escape_attr, escape_html};

#[derive(Default)]
struct DirNode {
    dirs: BTreeMap<String, DirNode>,
    files: Vec<usize>,
    adds: u32,
    dels: u32,
}

impl DirNode {
    fn insert(&mut self, parts: &[&str], idx: usize, adds: u32, dels: u32) {
        self.adds += adds;
        self.dels += dels;
        match parts {
            [] | [_] => self.files.push(idx),
            [dir, rest @ ..] => self
                .dirs
                .entry((*dir).to_string())
                .or_default()
                .insert(rest, idx, adds, dels),
        }
    }
}

/// Merge single-child directory chains (`nix` → `services` → `foo` ⇒ `nix/services/foo`).
fn compress(name: String, mut node: DirNode) -> (String, DirNode) {
    if node.files.is_empty() && node.dirs.len() == 1 {
        if let Some((child_name, child)) = node.dirs.pop_first() {
            return compress(format!("{name}/{child_name}"), child);
        }
    }
    (name, node)
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn render_tree_node(node: DirNode, files: &[FileChange], out: &mut String) {
    out.push_str(r#"<ul class="neo-tree-list">"#);
    for (name, child) in node.dirs {
        let (name, child) = compress(name, child);
        out.push_str(&format!(
            r#"<li><details open class="neo-tree-dir"><summary>{ICON_CHEVRON}{ICON_FOLDER}<span class="neo-tree-name" title="{t}">{n}</span>{c}</summary>"#,
            t = escape_attr(&name),
            n = escape_html(&name),
            c = counts_html(child.adds, child.dels, false),
        ));
        render_tree_node(child, files, out);
        out.push_str("</details></li>");
    }
    for idx in node.files {
        let Some(f) = files.get(idx) else {
            continue;
        };
        let title = match &f.old_path {
            Some(old) => format!("{}: {} → {}", f.status.label(), old, f.path),
            None => format!("{}: {}", f.status.label(), f.path),
        };
        out.push_str(&format!(
            r#"<li><button type="button" class="neo-tree-file" data-status="{st}" data-diff-jump="{id}" title="{title}">{badge}<span class="neo-tree-name">{name}</span>{counts}</button></li>"#,
            st = f.status.css_key(),
            id = file_id(idx),
            title = escape_attr(&title),
            badge = status_badge(f.status),
            name = escape_html(base_name(&f.path)),
            counts = counts_html(f.additions, f.deletions, f.binary),
        ));
    }
    out.push_str("</ul>");
}

pub(super) fn render_tree(set: &ChangeSet) -> String {
    let mut root = DirNode::default();
    for (i, f) in set.files.iter().enumerate() {
        let parts: Vec<&str> = f.path.split('/').filter(|p| !p.is_empty()).collect();
        root.insert(&parts, i, f.additions, f.deletions);
    }
    let mut s = String::from(
        r#"<nav class="neo-tree" aria-label="Changed files"><div class="neo-tree-title">Files</div>"#,
    );
    render_tree_node(root, &set.files, &mut s);
    s.push_str("</nav>");
    s
}
