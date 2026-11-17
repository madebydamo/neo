//! The semantic settings summary and the flake input updates above the files.
use super::super::flake_lock::InputUpdate;
use super::super::settings_semantic::{SettingsDiff, Transition, ValueChange};
use super::{plural, ICON_LOCK};
use crate::commands::web::util::{core_section_ok, escape_attr, escape_html, service_name_ok};

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

pub(super) fn render_semantic(sem: &Result<SettingsDiff, String>) -> String {
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

pub(super) fn render_inputs(inputs: &[InputUpdate]) -> String {
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
