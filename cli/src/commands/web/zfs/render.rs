//! Shared snapshot list markup for the option pane ([`super::service`]) and the
//! versioning tab ([`super::data`]), so both lists look and behave the same.
use super::super::util::{escape_attr, escape_html};
use super::{format_age, format_epoch_utc, human_bytes, snapshot_kind, Snapshot};

/// Chevron used by the collapsible `<details>` summaries.
pub const CHEVRON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3.5 h-3.5 shrink-0 text-base-content/50 transition-transform group-open:rotate-90" aria-hidden="true"><path fill-rule="evenodd" d="M7.2 14.8a.75.75 0 010-1.06L10.94 10 7.2 6.26a.75.75 0 111.06-1.06l4.27 4.27a.75.75 0 010 1.06L8.26 14.8a.75.75 0 01-1.06 0z" clip-rule="evenodd"/></svg>"#;

const CAMERA: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" class="w-4 h-4" aria-hidden="true"><path d="M4 8.5A1.5 1.5 0 015.5 7h2.3l1.5-2h5.4l1.5 2h2.3A1.5 1.5 0 0120 8.5v9a1.5 1.5 0 01-1.5 1.5h-13A1.5 1.5 0 014 17.5v-9z"/><circle cx="12" cy="13" r="3.2"/></svg>"#;

/// Button classes for a secondary (non-destructive) row / header action.
pub const BTN_SECONDARY: &str = "btn btn-sm btn-ghost bg-base-100 border-base-300";

const PIN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4" aria-hidden="true"><path d="M9 4h6l-1 6 3 3v1H7v-1l3-3-1-6z"/><path d="M12 14v6"/></svg>"#;
const PIN_FILLED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4" aria-hidden="true"><path d="M9 4h6l-1 6 3 3v1H7v-1l3-3-1-6z"/><path d="M12 14v6" fill="none"/></svg>"#;
const PENCIL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16v4z"/><path d="M13.5 6.5l4 4"/></svg>"#;
const TRASH: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4" aria-hidden="true"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/></svg>"#;

/// Icon-only row button (pin / comment / delete).
const BTN_ICON: &str = "btn btn-sm btn-ghost btn-square";

/// One snapshot row: icon, time + kind badge (+ pin, comment),
/// `age · size{meta}` and actions. `meta_html` is appended to the second line
/// (already escaped HTML). `highlight` tints the icon (snapshots this scope
/// created). The empty `.snap-edit` slot receives the comment form.
pub fn snapshot_row(
    s: &Snapshot,
    now: i64,
    meta_html: &str,
    actions_html: &str,
    highlight: bool,
) -> String {
    let (kind, badge) = snapshot_kind(&s.name);
    let icon_cls = if highlight {
        "bg-info/15 text-info"
    } else {
        "bg-base-200 text-base-content/60"
    };
    let pin = if s.pinned {
        r#"<span class="badge badge-xs badge-soft badge-primary" title="Pinned: never deleted automatically">pinned</span>"#
    } else {
        ""
    };
    let comment = s
        .comment
        .as_deref()
        .map(|c| {
            format!(
                r#"<span class="text-xs font-medium text-base-content/80 truncate max-w-full" title="{}">{}</span>"#,
                escape_attr(c),
                escape_html(c)
            )
        })
        .unwrap_or_default();
    format!(
        r#"<li class="flex flex-col sm:flex-row sm:flex-wrap sm:items-center gap-2 sm:gap-x-3 px-3 sm:px-4 py-2.5">
<div class="flex items-center gap-3 min-w-0 flex-1">
<span class="w-9 h-9 shrink-0 rounded-xl {icon_cls} flex items-center justify-center">{CAMERA}</span>
<div class="min-w-0"><div class="flex items-center gap-1.5 flex-wrap"><span class="text-sm font-semibold tabular-nums" title="{full}">{when}</span><span class="badge badge-xs {badge}">{kind}</span>{pin}{comment}</div>
<div class="text-xs text-base-content/55 truncate">{age} · <span title="Space held only by this snapshot">{used}</span>{meta_html}</div></div>
</div>
<div class="flex flex-wrap items-center gap-1.5 sm:justify-end pl-12 sm:pl-0">{actions_html}</div>
<div class="snap-edit basis-full empty:hidden"></div></li>"#,
        kind = escape_html(&kind),
        full = escape_attr(&s.full),
        when = escape_html(&format_epoch_utc(s.creation)),
        age = format_age(now, s.creation),
        used = human_bytes(s.used),
    )
}

/// htmx wiring shared by the pin / comment / delete buttons of one list.
pub struct ManageCtx<'a> {
    /// Route prefix: `/versioning/zfs` or `/service/<svc>/snapshots`.
    pub base: &'a str,
    /// Target/swap attributes for the POSTs (whole card, or `hx-swap="none"` + OOB).
    pub hx: &'a str,
    /// Another operation runs: show the buttons disabled.
    pub busy: bool,
}

/// Query string identifying a snapshot (`ds=` only when the route needs it).
pub fn snap_query(ds: Option<&str>, snap: &str) -> String {
    let snap = urlencode(snap);
    match ds {
        Some(ds) => format!("ds={}&snap={snap}", urlencode(ds)),
        None => format!("snap={snap}"),
    }
}

/// Minimal query escaping for validated dataset / snapshot names.
pub fn urlencode(s: &str) -> String {
    s.replace('%', "%25")
        .replace(':', "%3A")
        .replace('/', "%2F")
}

/// Pin toggle, comment and delete buttons for one row. `query` comes from [`snap_query`].
pub fn manage_actions(s: &Snapshot, query: &str, ctx: &ManageCtx) -> String {
    let base = ctx.base;
    let hx = ctx.hx;
    let q = escape_attr(query);
    if ctx.busy {
        return format!(
            r#"<button type="button" class="{BTN_ICON} btn-disabled" disabled title="Another operation is running">{}</button>"#,
            if s.pinned { PIN_FILLED } else { PIN }
        );
    }
    let when = format_epoch_utc(s.creation);
    let pin = if s.pinned {
        let confirm = if s.defer_destroy {
            format!(
                r#" hx-confirm="{}""#,
                escape_attr(&format!(
                    "Unpin {full} ({when})?\n\n\
                     • The automatic retention already expired this snapshot.\n\
                     • Unpinning DELETES it right away. This cannot be undone.",
                    full = s.full
                ))
            )
        } else {
            String::new()
        };
        format!(
            r#"<button type="button" class="{BTN_ICON} text-primary" hx-post="{base}/unpin?{q}" {hx} hx-disabled-elt="this"{confirm} title="Pinned — click to unpin (automatic retention may delete it again)" aria-label="Unpin">{PIN_FILLED}</button>"#
        )
    } else {
        format!(
            r#"<button type="button" class="{BTN_ICON} text-base-content/50" hx-post="{base}/pin?{q}" {hx} hx-disabled-elt="this" title="Pin — never delete this snapshot automatically" aria-label="Pin">{PIN}</button>"#
        )
    };
    let comment = format!(
        r#"<button type="button" class="{BTN_ICON} text-base-content/50" hx-get="{base}/comment?{q}" hx-target="next .snap-edit" hx-swap="innerHTML" title="Edit comment" aria-label="Edit comment">{PENCIL}</button>"#
    );
    let delete = if s.pinned || s.userrefs > 0 {
        format!(
            r#"<button type="button" class="{BTN_ICON} btn-disabled" disabled title="{}" aria-label="Delete">{TRASH}</button>"#,
            if s.pinned {
                "Pinned — unpin before deleting"
            } else {
                "Held by another tool (zfs holds)"
            }
        )
    } else {
        let confirm = format!(
            "Delete snapshot {full} ({when})?\n\n\
             • The snapshot is destroyed; this cannot be undone.\n\
             • Live data and other snapshots are not touched.",
            full = s.full,
        );
        format!(
            r#"<button type="button" class="{BTN_ICON} text-base-content/50 hover:text-error" hx-post="{base}/delete?{q}" {hx} hx-disabled-elt="this" hx-confirm="{c}" title="Delete this snapshot" aria-label="Delete">{TRASH}</button>"#,
            c = escape_attr(&confirm),
        )
    };
    format!("{pin}{comment}{delete}")
}

/// Inline comment editor, swapped into the row's `.snap-edit` slot.
pub fn comment_form(s: &Snapshot, query: &str, ctx: &ManageCtx) -> String {
    format!(
        r#"<form class="flex flex-wrap items-center gap-1.5 sm:pl-12" hx-post="{base}/comment?{q}" {hx}>
<input type="text" name="comment" maxlength="{max}" value="{cur}" placeholder="e.g. Known good after setup" class="input input-sm flex-1 min-w-40" aria-label="Snapshot comment" autofocus>
<button type="submit" class="btn btn-sm btn-primary">Save</button>
<button type="button" class="btn btn-sm btn-ghost" data-neo-remove-closest="form">Cancel</button>
</form>"#,
        base = ctx.base,
        hx = ctx.hx,
        q = escape_attr(query),
        max = super::COMMENT_MAX,
        cur = escape_attr(s.comment.as_deref().unwrap_or("")),
    )
}

/// Collapsible list card: `<details>` with title, count and dataset, then rows
/// (newest first, at most `max_rows`) or an empty state.
pub fn snapshot_list(
    title: &str,
    ds: &str,
    open: bool,
    total: usize,
    rows: &[String],
    max_rows: usize,
) -> String {
    let mut html = format!(
        r#"<details class="group rounded-box border border-base-300 bg-base-100 overflow-hidden"{open}>
<summary class="cursor-pointer list-none px-3 sm:px-4 py-2.5 flex items-center gap-2 border-b border-transparent group-open:border-base-300">{CHEVRON}<span class="text-sm font-semibold">{title}</span><span class="text-xs text-base-content/40 tabular-nums">{total}</span><span class="font-mono text-[10px] text-base-content/40 ml-auto truncate hidden sm:inline" title="ZFS dataset">{ds}</span></summary>"#,
        open = if open { " open" } else { "" },
        title = escape_html(title),
        ds = escape_html(ds),
    );
    if rows.is_empty() {
        html.push_str(
            r#"<div class="px-4 py-6 text-center text-xs text-base-content/50">No snapshots yet.</div>"#,
        );
    } else {
        html.push_str(r#"<ul class="divide-y divide-base-300/70 max-h-[28rem] overflow-auto">"#);
        for r in rows {
            html.push_str(r);
        }
        html.push_str("</ul>");
        if total > max_rows {
            html.push_str(&format!(
                r#"<div class="px-4 py-2 text-[11px] text-base-content/50 border-t border-base-300">{max_rows} newest of {total} shown.</div>"#
            ));
        }
    }
    html.push_str("</details>");
    html
}

/// "How restoring works" disclosure; `items_html` are `<li>` contents.
pub fn how_it_works(items_html: &[&str]) -> String {
    let items: String = items_html.iter().map(|i| format!("<li>{i}</li>")).collect();
    format!(
        r#"<details class="group rounded-box border border-base-300 bg-base-100 text-xs text-base-content/70">
<summary class="cursor-pointer list-none px-3 sm:px-4 py-2.5 flex items-center gap-2 font-medium text-base-content/80">{CHEVRON}How restoring works</summary>
<ul class="px-3 sm:px-4 pb-3 pl-8 sm:pl-9 space-y-1 list-disc leading-relaxed">{items}</ul>
</details>"#
    )
}
