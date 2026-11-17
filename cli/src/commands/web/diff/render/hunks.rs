//! Per-file unified / split diff tables with folded context.
use super::super::intraline::{word_diff, Segment};
use super::super::model::{DiffLine, FileChange, FileStatus, Hunk, LineKind};
use super::{counts_html, file_id, plural, stat_bar, status_badge, ICON_CHEVRON, ICON_EXPAND};
use crate::commands::web::util::{escape_attr, escape_html};

/// Context lines kept next to a change when a long unchanged run is folded.
const FOLD_EDGE: usize = 3;
/// Minimum number of hidden lines worth a fold (shorter runs stay visible).
const FOLD_MIN_HIDDEN: usize = 4;
/// Files with more changed lines start collapsed.
const COLLAPSE_CHANGED_LINES: u32 = 400;
/// Hard cap of diff lines rendered per file.
const MAX_LINES_PER_FILE: usize = 2000;

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

pub(super) fn render_file(i: usize, f: &FileChange, budget: &mut usize) -> String {
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

#[cfg(test)]
mod tests {
    use super::super::tests::change;
    use super::super::MAX_LINES_TOTAL;
    use super::*;

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
}
