//! HTML for the changes preview: summary header, semantic settings summary,
//! file tree and per-file unified / split diffs.
//!
//! Interactivity (jump, expand context, view toggle, expand/collapse all) is done by
//! delegated handlers in `static/diff_view.js` keyed on `data-diff-*` attributes, so
//! the markup works whether it is swapped in by htmx or `innerHTML`.
mod hunks;
mod semantic;
mod tree;

use super::flake_lock::InputUpdate;
use super::model::{ChangeSet, FileStatus};
use super::settings_semantic::SettingsDiff;
use crate::commands::web::util::escape_html;
use hunks::render_file;
use semantic::{render_inputs, render_semantic};
use tree::render_tree;

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

pub(super) const ICON_CHEVRON: &str = r#"<svg class="neo-chev" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true"><path fill-rule="evenodd" d="M7.2 14.8a.75.75 0 010-1.06L10.94 10 7.2 6.26a.75.75 0 111.06-1.06l4.27 4.27a.75.75 0 010 1.06L8.26 14.8a.75.75 0 01-1.06 0z" clip-rule="evenodd"/></svg>"#;
pub(super) const ICON_FOLDER: &str = r#"<svg class="neo-tree-icon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" aria-hidden="true"><path d="M3 7.5A1.5 1.5 0 014.5 6h4l2 2h9A1.5 1.5 0 0121 9.5v8a1.5 1.5 0 01-1.5 1.5h-15A1.5 1.5 0 013 17.5z"/></svg>"#;
pub(super) const ICON_LOCK: &str = r#"<svg class="neo-lock" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 018 0v3"/></svg>"#;
pub(super) const ICON_EXPAND: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5" aria-hidden="true"><path d="M8 9l4-4 4 4M8 15l4 4 4-4"/></svg>"#;

pub(super) fn file_id(i: usize) -> String {
    format!("neo-diff-f{i}")
}

pub(super) fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("{n} {one}")
    } else {
        format!("{n} {many}")
    }
}

pub(super) fn status_badge(s: FileStatus) -> String {
    format!(
        r#"<span class="neo-st" data-status="{}" title="{}"><span aria-hidden="true">{}</span><span class="sr-only">{}</span></span>"#,
        s.css_key(),
        s.label(),
        s.letter(),
        s.label()
    )
}

pub(super) fn counts_html(adds: u32, dels: u32, binary: bool) -> String {
    if binary && adds == 0 && dels == 0 {
        return r#"<span class="neo-counts"><span class="neo-bin">bin</span></span>"#.to_string();
    }
    format!(
        r#"<span class="neo-counts"><span class="neo-add-n" aria-label="{adds} added lines">+{adds}</span><span class="neo-del-n" aria-label="{dels} removed lines">−{dels}</span></span>"#
    )
}

/// Five-block add/delete ratio bar (decorative; numbers are shown next to it).
pub(super) fn stat_bar(adds: u32, dels: u32) -> String {
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
    use super::super::model::{FileChange, LineKind};
    use super::super::parse::parse_patch;
    use super::super::settings_semantic::diff_settings_toml;
    use super::*;

    pub(super) fn change(path: &str, status: FileStatus, patch: &str) -> FileChange {
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
