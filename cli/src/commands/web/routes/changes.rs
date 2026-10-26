use std::sync::Arc;

use rocket::response::content::RawHtml;
use rocket::{get, post, State};

use crate::commands::web::action_bar::{action_bar_dynamic_element, broadcast_action_bar};
use crate::commands::web::diff::{render_changes, RenderOptions};
use crate::commands::web::git::{collect_changes, lock_updates, settings_semantic, DiffRange};
use crate::commands::web::locks::{try_lock, Blocked};
use crate::commands::web::settings::discard_pending_changes;
use crate::commands::web::trigger::trigger_activation;
use crate::commands::web::types::AppConfig;
use crate::commands::web::util::{alert_html, changes_actions_row, config_dir, AlertKind};
use crate::utils::locks::{LockSpec, OpInfo};

#[get("/changes/action-bar")]
pub fn changes_action_bar(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    RawHtml(action_bar_dynamic_element(config, false))
}

const ALL_APPLIED: &str = r#"<div class="flex flex-col items-center text-center gap-2 py-8"><span class="w-10 h-10 rounded-full bg-success/15 text-success flex items-center justify-center"><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg></span><div class="font-semibold text-sm">Everything is applied</div><div class="text-xs text-base-content/60">No pending changes in the working tree.</div></div>"#;

/// Pending-changes preview: file tree, semantic settings summary and full diffs of
/// every changed file (tracked and untracked) against the last activation (`HEAD`).
#[get("/changes/summary")]
pub fn changes_summary(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let dir = config_dir(&config.settings_path);
    let set = match collect_changes(&dir, DiffRange::Worktree) {
        Ok(set) => set,
        Err(e) => {
            return RawHtml(format!(
                "{}{}",
                alert_html(
                    AlertKind::Error,
                    &format!("Could not read pending changes: {e:#}")
                ),
                changes_actions_row()
            ))
        }
    };
    if set.is_empty() {
        return RawHtml(ALL_APPLIED.to_string());
    }
    let semantic = settings_semantic(&dir, &set, DiffRange::Worktree);
    let inputs = lock_updates(&dir, &set, DiffRange::Worktree);
    let html = render_changes(
        &set,
        &RenderOptions {
            subtitle: Some(
                "Saved but not yet activated. Activate to apply, or discard to return to the last applied state.",
            ),
            semantic: semantic.as_ref(),
            inputs: &inputs,
        },
    );
    RawHtml(format!("{html}{}", changes_actions_row()))
}

/// Shared with `/actions/reset`: restore the worktree to `HEAD`.
/// Not while an activation / update commits or rewrites the config repo.
pub fn discard_response(config: &AppConfig) -> Result<RawHtml<String>, Blocked> {
    let _lock = try_lock(
        &LockSpec::system_shared(),
        OpInfo::new("settings", "Discarding changes"),
    )?;
    Ok(RawHtml(match discard_pending_changes(config) {
        Ok(()) => alert_html(
            AlertKind::Success,
            "Pending changes discarded — back to the last committed configuration.",
        ),
        Err(e) => alert_html(AlertKind::Error, &format!("Discard failed: {e}")),
    }))
}

#[post("/changes/revert")]
pub fn revert_settings(config: &State<Arc<AppConfig>>) -> Result<RawHtml<String>, Blocked> {
    discard_response(config)
}

/// Shared with `/actions/activate`: trigger activation oneshot and refresh action bar.
pub fn apply_or_activate(config: &AppConfig) -> Result<RawHtml<String>, Blocked> {
    let html = trigger_activation()?;
    broadcast_action_bar(config);
    Ok(html)
}

#[post("/changes/apply")]
pub fn apply_settings(config: &State<Arc<AppConfig>>) -> Result<RawHtml<String>, Blocked> {
    apply_or_activate(config)
}
