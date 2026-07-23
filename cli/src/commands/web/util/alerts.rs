use super::escape::escape_html;

/// DaisyUI alert fragment kinds.
#[derive(Clone, Copy, Debug)]
pub enum AlertKind {
    Success,
    Error,
    Info,
}

impl AlertKind {
    fn class(self) -> &'static str {
        match self {
            AlertKind::Success => "alert-success",
            AlertKind::Error => "alert-error",
            AlertKind::Info => "alert-info",
        }
    }
}

/// Build a small daisyUI alert; `msg` is HTML-escaped.
pub fn alert_html(kind: AlertKind, msg: &str) -> String {
    format!(
        r#"<div class="alert {} text-sm">{}</div>"#,
        kind.class(),
        escape_html(msg)
    )
}

/// Shared Revert / Apply button row for the changes dialog.
pub fn changes_actions_row() -> &'static str {
    r##"<div class="mt-4 flex flex-nowrap items-center justify-end gap-2" data-dialog-actions>
  <button type="button" hx-post="/changes/revert" hx-target="#changes-body" hx-swap="innerHTML" class="btn btn-sm btn-ghost">Revert</button>
  <button type="button" hx-post="/changes/apply" hx-target="#changes-body" hx-swap="innerHTML" hx-confirm="Run full activation (write-flake + nixos-rebuild)? This can take several minutes." class="btn btn-sm btn-error">Apply (activate)</button>
</div>"##
}
