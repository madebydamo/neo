use super::super::locks::{system_lock_attr, LockMode};
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
pub fn changes_actions_row() -> String {
    let lk = system_lock_attr(LockMode::Exclusive);
    format!(
        r##"<div class="mt-4 flex flex-nowrap items-center justify-end gap-2" data-dialog-actions>
  <button type="button" hx-post="/changes/revert" hx-target="#changes-body" hx-swap="innerHTML" class="btn btn-sm btn-ghost" hx-confirm="Discard all uncommitted changes and return to the last committed configuration?">Discard changes</button>
  <button type="button" hx-post="/changes/apply"{lk} hx-target="#changes-body" hx-swap="innerHTML" hx-confirm="Run full activation (write-flake + nixos-rebuild)? This can take several minutes." class="btn btn-sm btn-primary">Activate now</button>
</div>"##
    )
}
