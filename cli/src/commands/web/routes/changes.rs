use std::path::PathBuf;
use std::sync::Arc;

use rocket::response::content::RawHtml;
use rocket::{get, post, State};
use toml_edit::DocumentMut;

use crate::commands::paste_settings::paste_settings;
use crate::commands::web::action_bar::{
    broadcast_action_bar, render_action_bar_dynamic_inner, render_pending_changes_html,
    render_reset_button_html,
};
use crate::commands::web::git_ops::{
    get_settings_toml_diff, settings_toml_has_diff, worktree_changed_and_summary,
};
use crate::commands::web::structs::AppConfig;
use crate::commands::web::trigger::trigger_activation;
use crate::commands::web::util::{config_dir, escape_html};

#[get("/changes/action-bar")]
pub fn changes_action_bar(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    RawHtml(format!(
        r#"<div id="action-bar-dynamic" class="flex items-center gap-2">{}</div>"#,
        render_action_bar_dynamic_inner(&config)
    ))
}

#[get("/changes/indicator")]
pub fn changes_indicator(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    RawHtml(render_pending_changes_html(&config))
}

#[get("/changes/reset-button")]
pub fn reset_button(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    RawHtml(render_reset_button_html(&config))
}

#[get("/changes/summary")]
pub fn changes_summary(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let body = if settings_toml_has_diff(&config) {
        let diff = get_settings_toml_diff(&config);
        let esc = escape_html(&diff);
        format!(
            "<div class=\"mb-2 text-warning text-sm\">Pending changes to settings.toml (git diff)</div>\
             <pre class=\"text-xs overflow-auto max-h-[50vh] bg-base-300 p-2 rounded whitespace-pre\">{}</pre>\
             <div class=\"mt-4 flex flex-nowrap items-center justify-end gap-2\" data-dialog-actions>\
               <button type=\"button\" hx-post=\"/changes/revert\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" class=\"btn btn-sm btn-ghost\">Revert</button>\
               <button type=\"button\" hx-post=\"/changes/apply\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" hx-confirm=\"Run full activation (write-flake + nixos-rebuild)? This can take several minutes.\" class=\"btn btn-sm btn-error\">Apply (activate)</button>\
             </div>",
            esc
        )
    } else {
        let (changed, summary) = worktree_changed_and_summary(&config);
        if changed {
            let esc = escape_html(&summary);
            format!(
                "<div class=\"mb-2 text-warning text-sm\">Other files changed in working tree</div>\
                 <pre class=\"text-xs overflow-auto max-h-[50vh] bg-base-300 p-2 rounded whitespace-pre\">{}</pre>\
                 <div class=\"mt-4 flex flex-nowrap items-center justify-end gap-2\" data-dialog-actions>\
                   <button type=\"button\" hx-post=\"/changes/revert\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" class=\"btn btn-sm btn-ghost\">Revert</button>\
                   <button type=\"button\" hx-post=\"/changes/apply\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" hx-confirm=\"Run full activation (write-flake + nixos-rebuild)? This can take several minutes.\" class=\"btn btn-sm btn-error\">Apply (activate)</button>\
                 </div>",
                esc
            )
        } else {
            "<div class=\"text-sm\">Working tree clean. No pending changes.</div>".to_string()
        }
    };
    RawHtml(body)
}

#[post("/changes/revert")]
pub fn revert_settings(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let dir = config_dir(&config);
    let dir_str = dir.to_str().unwrap_or(".");
    let source = PathBuf::from("/etc/neo/settings.toml");
    let dummy = DocumentMut::new();
    let res = paste_settings(dir_str, &source, &dummy, false, &config.nix_cmd);
    if res.is_ok() {
        let ev = config.evaluator.clone();
        tokio::spawn(async move {
            let mut g = ev.lock().await;
            let _ = g.refresh().await;
        });
        broadcast_action_bar(&config);
    }
    match res {
        Ok(()) => RawHtml(
            "<div class=\"alert alert-success text-sm\">Reverted via paste-settings. Close and reload options to see state.</div>"
                .to_string(),
        ),
        Err(e) => RawHtml(format!(
            "<div class=\"alert alert-error text-sm\">Revert failed: {}</div>",
            e
        )),
    }
}

#[post("/changes/apply")]
pub fn apply_settings(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let html = trigger_activation(&config);
    broadcast_action_bar(&config);
    html
}
