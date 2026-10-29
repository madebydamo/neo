//! HTML for the changes preview: summary header, semantic settings summary,
//! file tree and per-file unified / split diffs.
//!
//! Interactivity (jump, expand context, view toggle, expand/collapse all) is done by
//! delegated handlers in `static/diff_view.js` keyed on `data-diff-*` attributes, so
//! the markup works whether it is swapped in by htmx or `innerHTML`.
use std::collections::BTreeMap;

use super::flake_lock::InputUpdate;
use super::intraline::{word_diff, Segment};
use super::model::{ChangeSet, DiffLine, FileChange, FileStatus, Hunk, LineKind};
use super::settings_semantic::{SettingsDiff, Transition, ValueChange};
use crate::commands::web::util::{core_section_ok, escape_attr, escape_html, service_name_ok};

/// Context lines kept next to a change when a long unchanged run is folded.
const FOLD_EDGE: usize = 3;
/// Minimum number of hidden lines worth a fold (shorter runs stay visible).
const FOLD_MIN_HIDDEN: usize = 4;
/// Files with more changed lines start collapsed.
const COLLAPSE_CHANGED_LINES: u32 = 400;
/// Hard cap of diff lines rendered per file.
const MAX_LINES_PER_FILE: usize = 2000;
/// Total diff lines rendered across all files.
const MAX_LINES_TOTAL: usize = 10_000;

/// Options for [`render_changes`].
pub struct RenderOptions<'a> {
    /// Explanatory line under the summary header.
    pub subtitle: Option<&'a str>,
    /// Semantic `settings.toml` summary (`Err` = could not be computed).
    pub semantic: Option<&'a Result<SettingsDiff, String>>,
    /// Flake inputs updated in `flake.lock` (empty = section hidden).
    pub inputs: &'a [InputUpdate],
}

const ICON_CHEVRON: &str = r#"<svg class="neo-chev" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true"><path fill-rule="evenodd" d="M7.2 14.8a.75.75 0 010-1.06L10.94 10 7.2 6.26a.75.75 0 111.06-1.06l4.27 4.27a.75.75 0 010 1.06L8.26 14.8a.75.75 0 01-1.06 0z" clip-rule="evenodd"/></svg>"#;
const ICON_FOLDER: &str = r#"<svg class="neo-tree-icon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" aria-hidden="true"><path d="M3 7.5A1.5 1.5 0 014.5 6h4l2 2h9A1.5 1.5 0 0121 9.5v8a1.5 1.5 0 01-1.5 1.5h-15A1.5 1.5 0 013 17.5z"/></svg>"#;
const ICON_LOCK: &str = r#"<svg class="neo-lock" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 018 0v3"/></svg>"#;
const ICON_EXPAND: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5" aria-hidden="true"><path d="M8 9l4-4 4 4M8 15l4 4 4-4"/></svg>"#;

fn file_id(i: usize) -> String {
    format!("neo-diff-f{i}")
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("{n} {one}")
    } else {
        format!("{n} {many}")
    }
}

fn status_badge(s: FileStatus) -> String {
    format!(
        r#"<span class="neo-st" data-status="{}" title="{}"><span aria-hidden="true">{}</span><span class="sr-only">{}</span></span>"#,
        s.css_key(),
        s.label(),
        s.letter(),
        s.label()
    )
}

fn counts_html(adds: u32, dels: u32, binary: bool) -> String {
    if binary && adds == 0 && dels == 0 {
        return r#"<span class="neo-counts"><span class="neo-bin">bin</span></span>"#.to_string();
    }
    format!(
        r#"<span class="neo-counts"><span class="neo-add-n" aria-label="{adds} added lines">+{adds}</span><span class="neo-del-n" aria-label="{dels} removed lines">−{dels}</span></span>"#
    )
}

/// Five-block add/delete ratio bar (decorative; numbers are shown next to it).
fn stat_bar(adds: u32, dels: u32) -> String {
    let total = adds + dels;
    let (mut a, mut d) = (0u32, 0u32);
    if total > 0 {
        a = ((f64::from(adds) / f64::from(total)) * 5.0).round() as u32;
        d = if dels > 0 { 5 - a } else { 0 };
        if adds > 0 && a == 0 {
            (a, d) = (1, 4);
        }
        if dels > 0 && d == 0 {
            (a, d) = (4, 1);
        }
    }
    let mut s = String::from(r#"<span class="neo-statbar" aria-hidden="true">"#);
    for i in 0..5 {
        let k = if i < a {
            "add"
        } else if i < a + d {
            "del"
        } else {
            "none"
        };
        s.push_str(&format!(r#"<i data-k="{k}"></i>"#));
    }
    s.push_str("</span>");
    s
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(set: &ChangeSet, opts: &RenderOptions) -> String {
    let n = set.files.len();
    let (adds, dels) = (set.additions(), set.deletions());
    let subtitle = opts
        .subtitle
        .map(|s| format!(r#"<p class="neo-changes-sub">{}</p>"#, escape_html(s)))
        .unwrap_or_default();
    format!(
        r#"<div class="neo-changes-head">
  <div class="neo-changes-summary" role="status">
    <span class="neo-unit-dot" data-state="partial" aria-hidden="true"></span>
    <span class="font-semibold">{files} changed</span>
    <span class="neo-counts neo-counts-lg"><span class="neo-add-n">+{adds}</span><span class="neo-del-n">−{dels}</span></span>
    {bar}
  </div>
  <div class="neo-changes-tools">
    <div class="join neo-view-toggle" role="group" aria-label="Diff layout">
      <button type="button" class="btn btn-xs join-item" data-diff-view="unified" aria-pressed="true">Unified</button>
      <button type="button" class="btn btn-xs join-item" data-diff-view="split" aria-pressed="false">Split</button>
    </div>
    <button type="button" class="btn btn-xs btn-ghost" data-diff-toggle-all="open" title="Expand all files">Expand all</button>
    <button type="button" class="btn btn-xs btn-ghost" data-diff-toggle-all="close" title="Collapse all files">Collapse all</button>
  </div>
  {subtitle}
</div>"#,
        files = plural(n, "file", "files"),
        bar = stat_bar(adds, dels),
    )
}

// ---------------------------------------------------------------------------
// Semantic settings summary
// ---------------------------------------------------------------------------

/// Service / section name linking to its settings pane (`{base}/{name}`), or plain
/// text when the name is not a valid pane id.
fn pane_link(base: &str, name: &str, linkable: bool) -> String {
    if !linkable {
        return format!(r#"<span class="neo-sem-name">{}</span>"#, escape_html(name));
    }
    let url = escape_attr(&format!("{base}/{name}"));
    format!(
        r##"<a class="neo-sem-name" href="{url}" hx-get="{url}" hx-target="#config-content" hx-swap="innerHTML" hx-push-url="true" data-diff-close-modal title="{title}">{label}</a>"##,
        title = escape_attr(&format!("Open {name} settings")),
        label = escape_html(name),
    )
}

fn value_cell(v: Option<&str>, class: &str, secret: bool) -> String {
    match v {
        None => format!(r#"<td class="{class}"><span class="neo-sem-unset">not set</span></td>"#),
        Some(v) if secret => format!(
            r#"<td class="{class}"><span class="neo-sem-secret" title="Secret value hidden">{ICON_LOCK}{}</span></td>"#,
            escape_html(v)
        ),
        Some(v) => format!(
            r#"<td class="{class}"><code>{}</code></td>"#,
            escape_html(v)
        ),
    }
}

fn changes_table(changes: &[ValueChange]) -> String {
    if changes.is_empty() {
        return String::new();
    }
    let mut s = String::from(
        r#"<table class="neo-sem-table"><thead class="sr-only"><tr><th scope="col">Option</th><th scope="col">Before</th><th scope="col"></th><th scope="col">After</th></tr></thead><tbody>"#,
    );
    for c in changes {
        let kind = match (&c.old, &c.new) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            _ => "changed",
        };
        s.push_str(&format!(
            r#"<tr data-kind="{kind}"><th scope="row"><code>{path}</code></th>{old}<td class="neo-sem-arrow" aria-label="changed to">→</td>{new}</tr>"#,
            path = escape_html(&c.path),
            old = value_cell(c.old.as_deref(), "neo-sem-old", c.secret),
            new = value_cell(c.new.as_deref(), "neo-sem-new", c.secret),
        ));
    }
    s.push_str("</tbody></table>");
    s
}

fn render_semantic(sem: &Result<SettingsDiff, String>) -> String {
    let diff = match sem {
        Err(e) => {
            return format!(
                r#"<section class="neo-sem"><div class="neo-sem-title">Service changes</div><p class="neo-sem-error">Could not summarize settings.toml: {}</p></section>"#,
                escape_html(e)
            )
        }
        Ok(d) if d.is_empty() => {
            let text = match &d.secrets_note {
                Some(note) => escape_html(note),
                None => {
                    "No setting or secret value changed — only formatting, comments or encryption."
                        .to_string()
                }
            };
            return format!(
                r#"<section class="neo-sem"><div class="neo-sem-title">Service changes</div><p class="neo-sem-empty">{text}</p></section>"#
            );
        }
        Ok(d) => d,
    };
    let mut s =
        String::from(r#"<section class="neo-sem" aria-label="Summary of settings changes">"#);
    if !diff.services.is_empty() {
        let enabled = diff
            .services
            .iter()
            .filter(|x| x.transition() == Transition::Enabled)
            .count();
        let disabled = diff
            .services
            .iter()
            .filter(|x| x.transition() == Transition::Disabled)
            .count();
        let mut meta = vec![plural(diff.services.len(), "service", "services")];
        if enabled > 0 {
            meta.push(format!("{enabled} enabled"));
        }
        if disabled > 0 {
            meta.push(format!("{disabled} disabled"));
        }
        s.push_str(&format!(
            r#"<div class="neo-sem-title">Service changes <span class="neo-sem-meta">{}</span></div><ul class="neo-sem-list">"#,
            escape_html(&meta.join(" · "))
        ));
        for svc in &diff.services {
            let badge = match svc.transition() {
                Transition::Enabled => {
                    r#"<span class="neo-sem-badge" data-kind="enabled"><span aria-hidden="true">+</span> Enabled</span>"#.to_string()
                }
                Transition::Disabled => {
                    r#"<span class="neo-sem-badge" data-kind="disabled"><span aria-hidden="true">−</span> Disabled</span>"#.to_string()
                }
                Transition::Unchanged => String::new(),
            };
            let n = svc.changes.len();
            let opt_badge = if n > 0 {
                format!(
                    r#"<span class="neo-sem-badge" data-kind="changed"><span aria-hidden="true">~</span> {}</span>"#,
                    plural(n, "option", "options")
                )
            } else {
                String::new()
            };
            let name = pane_link(
                "/configuration/option",
                &svc.name,
                service_name_ok(&svc.name),
            );
            s.push_str(&format!(
                r#"<li class="neo-sem-item"><div class="neo-sem-item-head">{name}{badge}{opt_badge}</div>{}</li>"#,
                changes_table(&svc.changes)
            ));
        }
        s.push_str("</ul>");
    }
    if !diff.sections.is_empty() {
        s.push_str(r#"<div class="neo-sem-title">Other settings</div><ul class="neo-sem-list">"#);
        for sec in &diff.sections {
            let name = pane_link("/configuration/core", &sec.name, core_section_ok(&sec.name));
            s.push_str(&format!(
                r#"<li class="neo-sem-item"><div class="neo-sem-item-head">{name}<span class="neo-sem-badge" data-kind="changed"><span aria-hidden="true">~</span> {}</span></div>{}</li>"#,
                plural(sec.changes.len(), "option", "options"),
                changes_table(&sec.changes)
            ));
        }
        s.push_str("</ul>");
    }
    if let Some(note) = &diff.secrets_note {
        s.push_str(&format!(
            r#"<p class="neo-sem-empty">{ICON_LOCK} {}</p>"#,
            escape_html(note)
        ));
    }
    s.push_str("</section>");
    s
}

// ---------------------------------------------------------------------------
// Flake input updates
// ---------------------------------------------------------------------------

fn render_inputs(inputs: &[InputUpdate]) -> String {
    let mut s = format!(
        r#"<section class="neo-sem" aria-label="Dependency updates"><div class="neo-sem-title">Dependency updates <span class="neo-sem-meta">{}</span></div><ul class="neo-sem-list">"#,
        plural(inputs.len(), "input", "inputs")
    );
    for u in inputs {
        let badge = match (&u.old, &u.new) {
            (None, Some(_)) => {
                r#"<span class="neo-sem-badge" data-kind="enabled"><span aria-hidden="true">+</span> Added</span>"#
            }
            (Some(_), None) => {
                r#"<span class="neo-sem-badge" data-kind="disabled"><span aria-hidden="true">−</span> Removed</span>"#
            }
            _ => {
                r#"<span class="neo-sem-badge" data-kind="changed"><span aria-hidden="true">↑</span> Updated</span>"#
            }
        };
        let mut rows = Vec::new();
        if u.old.is_some() || u.new.is_some() {
            rows.push(ValueChange {
                path: "revision".into(),
                old: u.old.as_ref().map(|r| r.rev.clone()),
                new: u.new.as_ref().map(|r| r.rev.clone()),
                secret: false,
            });
            let (od, nd) = (
                u.old.as_ref().and_then(|r| r.date.clone()),
                u.new.as_ref().and_then(|r| r.date.clone()),
            );
            if od.is_some() || nd.is_some() {
                rows.push(ValueChange {
                    path: "date".into(),
                    old: od,
                    new: nd,
                    secret: false,
                });
            }
        }
        s.push_str(&format!(
            r#"<li class="neo-sem-item"><div class="neo-sem-item-head"><span class="neo-sem-name">{}</span>{badge}</div>{}</li>"#,
            escape_html(&u.name),
            changes_table(&rows)
        ));
    }
    s.push_str("</ul></section>");
    s
}

// ---------------------------------------------------------------------------
// File tree
// ---------------------------------------------------------------------------

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

fn render_tree(set: &ChangeSet) -> String {
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

// ---------------------------------------------------------------------------
// Diff tables
// ---------------------------------------------------------------------------

/// Word-diff segments per line index (paired del-run / add-run lines only).
fn intraline_for(hunk: &Hunk) -> Vec<Option<Vec<Segment>>> {
    let lines = &hunk.lines;
    let mut out: Vec<Option<Vec<Segment>>> = vec![None; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind != LineKind::Del {
            i += 1;
            continue;
        }
        let del_start = i;
        while i < lines.len() && lines[i].kind == LineKind::Del {
            i += 1;
        }
        let add_start = i;
        while i < lines.len() && lines[i].kind == LineKind::Add {
            i += 1;
        }
        let pairs = (add_start - del_start).min(i - add_start);
        for k in 0..pairs {
            let (d, a) = (del_start + k, add_start + k);
            if let Some((l, r)) = word_diff(&lines[d].text, &lines[a].text) {
                out[d] = Some(l);
                out[a] = Some(r);
            }
        }
    }
    out
}

/// Which lines of a hunk are folded away (long unchanged runs, keeping edges).
fn fold_mask(lines: &[DiffLine]) -> Vec<bool> {
    let mut hidden = vec![false; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind != LineKind::Context {
            i += 1;
            continue;
        }
        let start = i;
        while i < lines.len() && lines[i].kind == LineKind::Context {
            i += 1;
        }
        let end = i;
        let keep_head = if start == 0 { 0 } else { FOLD_EDGE };
        let keep_tail = if end == lines.len() { 0 } else { FOLD_EDGE };
        let len = end - start;
        if len >= keep_head + keep_tail + FOLD_MIN_HIDDEN {
            for h in hidden
                .iter_mut()
                .take(end - keep_tail)
                .skip(start + keep_head)
            {
                *h = true;
            }
        }
    }
    hidden
}

fn code_html(line: &DiffLine, segs: Option<&Vec<Segment>>) -> String {
    let mut s = match segs {
        Some(segs) => segs
            .iter()
            .map(|sg| {
                if sg.changed {
                    format!(r#"<span class="neo-dw">{}</span>"#, escape_html(&sg.text))
                } else {
                    escape_html(&sg.text)
                }
            })
            .collect::<String>(),
        None => escape_html(&line.text),
    };
    if line.no_newline {
        s.push_str(r#"<span class="neo-nonl" title="No newline at end of file">no newline at end of file</span>"#);
    }
    s
}

fn ln_cell(n: Option<u32>) -> String {
    match n {
        Some(n) => format!(r#"<td class="neo-ln" data-ln="{n}"></td>"#),
        None => r#"<td class="neo-ln"></td>"#.to_string(),
    }
}

fn kind_class(k: LineKind) -> (&'static str, &'static str) {
    match k {
        LineKind::Add => ("add", "+"),
        LineKind::Del => ("del", "−"),
        LineKind::Context => ("ctx", " "),
    }
}

fn unified_row(line: &DiffLine, segs: Option<&Vec<Segment>>) -> String {
    let (cls, sign) = kind_class(line.kind);
    format!(
        r#"<tr class="neo-dl" data-k="{cls}">{}{}<td class="neo-sign">{sign}</td><td class="neo-code">{}</td></tr>"#,
        ln_cell(line.old_no),
        ln_cell(line.new_no),
        code_html(line, segs)
    )
}

fn split_half(line: Option<(&DiffLine, Option<&Vec<Segment>>)>, side_old: bool) -> String {
    match line {
        None => r#"<td class="neo-ln" data-k="empty"></td><td class="neo-sign" data-k="empty"></td><td class="neo-code" data-k="empty"></td>"#.to_string(),
        Some((l, segs)) => {
            let (cls, sign) = kind_class(l.kind);
            let n = if side_old { l.old_no } else { l.new_no };
            let ln = match n {
                Some(n) => format!(r#"<td class="neo-ln" data-k="{cls}" data-ln="{n}"></td>"#),
                None => format!(r#"<td class="neo-ln" data-k="{cls}"></td>"#),
            };
            format!(
                r#"{ln}<td class="neo-sign" data-k="{cls}">{sign}</td><td class="neo-code" data-k="{cls}">{}</td>"#,
                code_html(l, segs)
            )
        }
    }
}

/// Split-view rows for a run of lines (context → both sides; del/add runs paired).
fn split_rows(lines: &[(usize, &DiffLine)], segs: &[Option<Vec<Segment>>]) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < lines.len() {
        let (idx, l) = lines[i];
        if l.kind == LineKind::Context {
            s.push_str(&format!(
                r#"<tr class="neo-dl">{}{}</tr>"#,
                split_half(Some((l, segs[idx].as_ref())), true),
                split_half(Some((l, segs[idx].as_ref())), false)
            ));
            i += 1;
            continue;
        }
        let del_start = i;
        while i < lines.len() && lines[i].1.kind == LineKind::Del {
            i += 1;
        }
        let add_start = i;
        while i < lines.len() && lines[i].1.kind == LineKind::Add {
            i += 1;
        }
        let dels = &lines[del_start..add_start];
        let adds = &lines[add_start..i];
        for k in 0..dels.len().max(adds.len()) {
            let left = dels.get(k).map(|(ix, l)| (*l, segs[*ix].as_ref()));
            let right = adds.get(k).map(|(ix, l)| (*l, segs[*ix].as_ref()));
            s.push_str(&format!(
                r#"<tr class="neo-dl">{}{}</tr>"#,
                split_half(left, true),
                split_half(right, false)
            ));
        }
    }
    s
}

fn fold_row(hidden: usize, cols: usize) -> String {
    format!(
        r#"<tbody class="neo-fold"><tr><td colspan="{cols}"><button type="button" class="neo-fold-btn" data-diff-expand>{ICON_EXPAND}Show {} </button></td></tr></tbody>"#,
        plural(hidden, "unchanged line", "unchanged lines")
    )
}

fn hunk_header_row(h: &Hunk, gap: u32, cols: usize) -> String {
    let gap_html = if gap > 0 {
        format!(
            r#"<span class="neo-hunk-gap">⋯ {}</span>"#,
            plural(gap as usize, "unchanged line", "unchanged lines")
        )
    } else {
        String::new()
    };
    let section = if h.section.is_empty() {
        String::new()
    } else {
        format!(
            r#" <span class="neo-hunk-sec">{}</span>"#,
            escape_html(&h.section)
        )
    };
    format!(
        r#"<tbody><tr class="neo-hunk"><td colspan="{cols}">{gap_html}<span class="neo-hunk-range">@@ -{},{} +{},{} @@</span>{section}</td></tr></tbody>"#,
        h.old_start, h.old_len, h.new_start, h.new_len
    )
}

/// Render unified + split tables for one file. `budget` = remaining line allowance.
fn render_hunks(hunks: &[Hunk], budget: &mut usize) -> String {
    let mut uni = String::from(
        r#"<table class="neo-diff-table neo-unified-table"><colgroup><col class="neo-col-ln"><col class="neo-col-ln"><col class="neo-col-sign"><col></colgroup>"#,
    );
    let mut split = String::from(
        r#"<table class="neo-diff-table neo-split-table"><colgroup><col class="neo-col-ln"><col class="neo-col-sign"><col class="neo-col-half"><col class="neo-col-ln"><col class="neo-col-sign"><col class="neo-col-half"></colgroup>"#,
    );
    let mut rendered = 0usize;
    let mut skipped = 0usize;
    let mut prev_old_end: u32 = 1;
    for h in hunks {
        if rendered >= MAX_LINES_PER_FILE || *budget == 0 {
            skipped += h.lines.len();
            continue;
        }
        let gap = h.old_start.saturating_sub(prev_old_end);
        prev_old_end = h.old_start + h.old_len;
        uni.push_str(&hunk_header_row(h, gap, 4));
        split.push_str(&hunk_header_row(h, gap, 6));
        let segs = intraline_for(h);
        let hidden = fold_mask(&h.lines);
        let mut i = 0;
        while i < h.lines.len() {
            if rendered >= MAX_LINES_PER_FILE || *budget == 0 {
                skipped += h.lines.len() - i;
                break;
            }
            let fold = hidden[i];
            let start = i;
            while i < h.lines.len() && hidden[i] == fold {
                i += 1;
            }
            let group: Vec<(usize, &DiffLine)> = (start..i).map(|k| (k, &h.lines[k])).collect();
            rendered += group.len();
            *budget = budget.saturating_sub(group.len());
            let body_attr = if fold {
                r#" class="neo-fold-body" hidden"#
            } else {
                ""
            };
            if fold {
                uni.push_str(&fold_row(group.len(), 4));
                split.push_str(&fold_row(group.len(), 6));
            }
            uni.push_str(&format!("<tbody{body_attr}>"));
            for (k, l) in &group {
                uni.push_str(&unified_row(l, segs[*k].as_ref()));
            }
            uni.push_str("</tbody>");
            split.push_str(&format!("<tbody{body_attr}>"));
            split.push_str(&split_rows(&group, &segs));
            split.push_str("</tbody>");
        }
    }
    uni.push_str("</table>");
    split.push_str("</table>");
    let more = if skipped > 0 {
        format!(
            r#"<div class="neo-file-note-row">{} not shown (preview size limit).</div>"#,
            plural(skipped, "more line", "more lines")
        )
    } else {
        String::new()
    };
    format!(r#"<div class="neo-diff-scroll">{uni}{split}</div>{more}"#)
}

fn placeholder(text: &str) -> String {
    format!(
        r#"<div class="neo-file-placeholder">{}</div>"#,
        escape_html(text)
    )
}

fn render_file(i: usize, f: &FileChange, budget: &mut usize) -> String {
    let (dir, base) = match f.path.rfind('/') {
        Some(p) => (&f.path[..=p], &f.path[p + 1..]),
        None => ("", f.path.as_str()),
    };
    let rename = f
        .old_path
        .as_ref()
        .map(|o| {
            let sim = f
                .similarity
                .map(|s| format!(" ({s}% similar)"))
                .unwrap_or_default();
            format!(
                r#"<span class="neo-file-old" title="Previous path{sim}">{} →</span>"#,
                escape_html(o)
            )
        })
        .unwrap_or_default();
    let mode = f
        .mode_change
        .as_ref()
        .map(|(a, b)| {
            format!(
                r#"<span class="neo-file-tag">mode {} → {}</span>"#,
                escape_html(a),
                escape_html(b)
            )
        })
        .unwrap_or_default();
    let changed = f.additions + f.deletions;
    let large = changed > COLLAPSE_CHANGED_LINES;
    let large_tag = if large {
        r#"<span class="neo-file-tag">large diff</span>"#
    } else {
        ""
    };

    let body = if let Some(reason) = &f.omitted {
        placeholder(reason)
    } else if f.binary {
        placeholder("Binary file — contents not shown.")
    } else if f.hunks.is_empty() {
        let text = match f.status {
            FileStatus::Renamed | FileStatus::Copied => {
                "File moved without content changes.".to_string()
            }
            _ if f.mode_change.is_some() => "Only the file mode changed.".to_string(),
            FileStatus::Added | FileStatus::Untracked => "Empty file.".to_string(),
            FileStatus::Deleted => "Empty file deleted.".to_string(),
            _ => "No content changes.".to_string(),
        };
        placeholder(&text)
    } else if *budget == 0 {
        placeholder("Diff not shown — the preview size limit was reached.")
    } else {
        render_hunks(&f.hunks, budget)
    };

    format!(
        r#"<details class="neo-file" id="{id}" data-diff-file data-status="{st}"{open}>
<summary class="neo-file-head">{ICON_CHEVRON}{badge}{rename}<span class="neo-file-path" title="{title}"><span class="neo-file-dir">{dir}</span><span class="neo-file-base">{base}</span></span>{mode}{large_tag}<span class="neo-file-stats">{counts}{bar}</span></summary>
<div class="neo-file-body">{body}</div>
</details>"#,
        id = file_id(i),
        st = f.status.css_key(),
        open = if large { "" } else { " open" },
        badge = status_badge(f.status),
        title = escape_attr(&f.path),
        dir = escape_html(dir),
        base = escape_html(base),
        counts = counts_html(f.additions, f.deletions, f.binary),
        bar = stat_bar(f.additions, f.deletions),
    )
}

/// Full changes preview (header, semantic summary, tree, diffs).
pub fn render_changes(set: &ChangeSet, opts: &RenderOptions) -> String {
    let mut s = String::from(r#"<div class="neo-changes" data-neo-changes>"#);
    s.push_str(&render_header(set, opts));
    if let Some(sem) = opts.semantic {
        s.push_str(&render_semantic(sem));
    }
    if !opts.inputs.is_empty() {
        s.push_str(&render_inputs(opts.inputs));
    }
    s.push_str(r#"<div class="neo-changes-grid">"#);
    s.push_str(&render_tree(set));
    s.push_str(r#"<div class="neo-files">"#);
    let mut budget = MAX_LINES_TOTAL;
    for (i, f) in set.files.iter().enumerate() {
        s.push_str(&render_file(i, f, &mut budget));
    }
    s.push_str("</div></div></div>");
    s
}

#[cfg(test)]
mod tests {
    use super::super::flake_lock::LockedRev;
    use super::super::parse::parse_patch;
    use super::super::settings_semantic::diff_settings_toml;
    use super::*;

    fn change(path: &str, status: FileStatus, patch: &str) -> FileChange {
        let hunks = parse_patch(patch)
            .into_iter()
            .next()
            .map(|p| p.hunks)
            .unwrap_or_default();
        let adds = hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == LineKind::Add)
            .count() as u32;
        let dels = hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == LineKind::Del)
            .count() as u32;
        FileChange {
            path: path.into(),
            old_path: None,
            status,
            additions: adds,
            deletions: dels,
            binary: false,
            similarity: None,
            mode_change: None,
            hunks,
            omitted: None,
        }
    }

    #[test]
    fn renders_tree_diff_and_semantic() {
        let patch = "diff --git a/settings.toml b/settings.toml\n--- a/settings.toml\n+++ b/settings.toml\n@@ -1,2 +1,2 @@\n [core]\n-hostname = \"<old>\"\n+hostname = \"new\"\n";
        let set = ChangeSet {
            files: vec![
                change("nix/services/foo/default.nix", FileStatus::Added, ""),
                change("settings.toml", FileStatus::Modified, patch),
            ],
        };
        let sem = diff_settings_toml(
            "[services.a]\nenabled = false\n",
            "[services.a]\nenabled = true\n",
        )
        .map_err(|e| e.to_string());
        let html = render_changes(
            &set,
            &RenderOptions {
                subtitle: Some("sub"),
                semantic: Some(&sem),
                inputs: &[],
            },
        );
        assert!(html.contains("2 files changed"));
        assert!(html.contains(r#"data-diff-jump="neo-diff-f1""#));
        // Single-child dirs are compressed in the tree.
        assert!(html.contains(">nix/services/foo<"));
        // Escaped content and intra-line highlight.
        assert!(html.contains("&lt;old&gt;"));
        assert!(html.contains(r#"<span class="neo-dw">new</span>"#));
        assert!(html.contains(r#"href="/configuration/option/a""#));
        assert!(html.contains("Enabled"));
        assert!(html.contains(r#"data-ln="2""#));
        assert!(html.contains("neo-split-table"));
    }

    #[test]
    fn long_context_is_folded() {
        let mut patch = String::from("diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,29 +1,29 @@\n");
        for i in 0..14 {
            patch.push_str(&format!(" c{i}\n"));
        }
        patch.push_str("-x\n+y\n");
        for i in 0..14 {
            patch.push_str(&format!(" d{i}\n"));
        }
        let f = change("f", FileStatus::Modified, &patch);
        let mask = fold_mask(&f.hunks[0].lines);
        // Leading run keeps its last 3 lines, trailing run its first 3.
        assert_eq!(mask.iter().filter(|h| **h).count(), 11 + 11);
        assert!(!mask[11] && mask[10]);
        let mut budget = MAX_LINES_TOTAL;
        let html = render_hunks(&f.hunks, &mut budget);
        assert!(html.contains("Show 11 unchanged lines"));
        assert!(html.contains(r#"class="neo-fold-body" hidden"#));
    }

    #[test]
    fn placeholders_for_binary_and_rename() {
        let mut bin = change("img.png", FileStatus::Added, "");
        bin.binary = true;
        let mut ren = change("b.nix", FileStatus::Renamed, "");
        ren.old_path = Some("a.nix".into());
        let html = render_changes(
            &ChangeSet {
                files: vec![bin, ren],
            },
            &RenderOptions {
                subtitle: None,
                semantic: None,
                inputs: &[InputUpdate {
                    name: "high_sea".into(),
                    old: Some(LockedRev {
                        rev: "aaaaaaa".into(),
                        date: None,
                    }),
                    new: Some(LockedRev {
                        rev: "bbbbbbb".into(),
                        date: Some("2026-10-26".into()),
                    }),
                }],
            },
        );
        assert!(html.contains("Dependency updates"));
        assert!(html.contains(">high_sea<"));
        assert!(html.contains("<code>bbbbbbb</code>"));
        assert!(html.contains("<code>2026-10-26</code>"));
        assert!(html.contains("Binary file"));
        assert!(html.contains("moved without content changes"));
        assert!(html.contains("a.nix →"));
    }
}
