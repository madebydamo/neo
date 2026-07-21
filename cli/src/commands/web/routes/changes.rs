use std::sync::Arc;

use rocket::response::content::RawHtml;
use rocket::{get, post, State};

use crate::commands::web::action_bar::{action_bar_dynamic_element, broadcast_action_bar};
use crate::commands::web::git_ops::{dirty_state, get_settings_toml_diff};
use crate::commands::web::settings::restore_settings_from_applied;
use crate::commands::web::structs::AppConfig;
use crate::commands::web::trigger::trigger_activation;
use crate::commands::web::util::{alert_html, changes_actions_row, escape_html, AlertKind};

#[get("/changes/action-bar")]
pub fn changes_action_bar(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    RawHtml(action_bar_dynamic_element(&config, false))
}

#[get("/changes/summary")]
pub fn changes_summary(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let d = dirty_state(&config);
    let body = if d.settings_dirty {
        let diff = get_settings_toml_diff(&config);
        let esc = escape_html(&diff);
        format!(
            "<div class=\"mb-2 text-warning text-sm\">Pending changes to settings.toml (git diff)</div>\
             <pre class=\"text-xs overflow-auto max-h-[50vh] bg-base-300 p-2 rounded whitespace-pre\">{}</pre>\
             {}",
            esc,
            changes_actions_row()
        )
    } else if d.worktree_dirty {
        let esc = escape_html(&d.summary);
        format!(
            "<div class=\"mb-2 text-warning text-sm\">Other files changed in working tree</div>\
             <pre class=\"text-xs overflow-auto max-h-[50vh] bg-base-300 p-2 rounded whitespace-pre\">{}</pre>\
             {}",
            esc,
            changes_actions_row()
        )
    } else {
        "<div class=\"text-sm\">Working tree clean. No pending changes.</div>".to_string()
    };
    RawHtml(body)
}

#[post("/changes/revert")]
pub fn revert_settings(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    match restore_settings_from_applied(&config) {
        Ok(()) => RawHtml(alert_html(
            AlertKind::Success,
            "Reverted via paste-settings. Close and reload options to see state.",
        )),
        Err(e) => RawHtml(alert_html(
            AlertKind::Error,
            &format!("Revert failed: {}", e),
        )),
    }
}

/// Shared with `/actions/activate`: trigger activation oneshot and refresh action bar.
pub fn apply_or_activate(config: &AppConfig) -> RawHtml<String> {
    let html = trigger_activation(config);
    broadcast_action_bar(config);
    html
}

#[post("/changes/apply")]
pub fn apply_settings(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    apply_or_activate(&config)
}
