use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::activation;
use super::git_ops::{settings_toml_has_diff, worktree_changed_and_summary};
use super::structs::AppConfig;

/// Compact signature of action-bar state so the watcher only pushes on real changes.
fn action_bar_signature(config: &AppConfig) -> String {
    activation::gc_old_activations();
    let busy = config.eval_busy.load(Ordering::Relaxed);
    let act = activation::find_recent_in_progress_activation().unwrap_or_default();
    let upd = activation::find_recent_in_progress_update().unwrap_or_default();
    let dirty = settings_toml_has_diff(config) || worktree_changed_and_summary(config).0;
    format!("{busy}|{act}|{upd}|{dirty}")
}

pub fn render_nix_busy_html(config: &AppConfig) -> String {
    if config.eval_busy.load(Ordering::Relaxed) {
        r#"<span class="inline-flex items-center gap-1 text-[10px] text-info opacity-90" title="Nix evaluator working"><span class="loading loading-spinner loading-xs"></span><span class="hidden sm:inline">eval</span></span>"#.to_string()
    } else {
        String::new()
    }
}

pub fn render_pending_changes_html(config: &AppConfig) -> String {
    activation::gc_old_activations();
    if let Some(id) = activation::find_recent_in_progress_activation() {
        return format!(
            "<button class=\"btn btn-warning btn-xs animate-pulse\" onclick=\"var m=document.getElementById('changes-modal');m.querySelector('h3').textContent='Activation progress';m.showModal();htmx.ajax('GET','/activation/monitor/{}',{{target:'#changes-body',swap:'innerHTML'}})\">Activation — view</button>",
            id
        );
    }
    if let Some(id) = activation::find_recent_in_progress_update() {
        return format!(
            "<button class=\"btn btn-info btn-xs animate-pulse\" onclick=\"var m=document.getElementById('changes-modal');m.querySelector('h3').textContent='Update progress';m.showModal();htmx.ajax('GET','/update/monitor/{}',{{target:'#changes-body',swap:'innerHTML'}})\">Update — view</button>",
            id
        );
    }
    let (changed, _) = worktree_changed_and_summary(config);
    if changed {
        "<button class=\"btn btn-warning btn-xs\" onclick=\"var m=document.getElementById('changes-modal');m.querySelector('h3').textContent='Pending changes';m.showModal();htmx.ajax('GET','/changes/summary',{target:'#changes-body',swap:'innerHTML'})\">Changes — review</button>".to_string()
    } else {
        "<span class=\"text-[10px] opacity-40\">clean</span>".to_string()
    }
}

pub fn render_reset_button_html(config: &AppConfig) -> String {
    let dirty = settings_toml_has_diff(config) || worktree_changed_and_summary(config).0;
    if !dirty {
        return String::new();
    }
    // After-request only opens the modal; action-bar refresh is pushed over WS.
    // Use r## so embedded "#id" attributes do not terminate the raw string.
    r##"<button hx-post="/actions/reset" hx-target="#changes-body" hx-swap="innerHTML" hx-confirm="Reset settings from last applied (/etc/neo)?" hx-on::after-request="var m=document.getElementById('changes-modal');if(m){m.querySelector('h3').textContent='Reset';m.showModal();}" class="btn btn-xs btn-ghost">↩<span class="hidden sm:inline ml-1">Reset</span></button>"##.to_string()
}

/// Inner HTML of `#action-bar-dynamic` (appearing middle section: busy, pending, reset).
pub fn render_action_bar_dynamic_inner(config: &AppConfig) -> String {
    format!(
        r#"{}{}{}"#,
        render_nix_busy_html(config),
        render_pending_changes_html(config),
        render_reset_button_html(config),
    )
}

/// Full OOB fragment for the action bar middle section (htmx ws extension applies it).
pub fn action_bar_oob_fragment(config: &AppConfig) -> String {
    format!(
        r#"<div id="action-bar-dynamic" class="flex items-center gap-2" hx-swap-oob="true">{}</div>"#,
        render_action_bar_dynamic_inner(config)
    )
}

pub fn broadcast_action_bar(config: &AppConfig) {
    let _ = config.unit_updates.send(action_bar_oob_fragment(config));
}

/// Background task: detect action-bar state changes and push OOB HTML to WS clients.
pub fn start_action_bar_watcher(config: Arc<AppConfig>) {
    tokio::spawn(async move {
        let mut last = String::new();
        loop {
            // Cheap loop: busy is atomic; git/activation checked each tick. Only send on change.
            let sig = action_bar_signature(&config);
            if sig != last {
                last = sig;
                broadcast_action_bar(&config);
            }
            tokio::time::sleep(Duration::from_millis(750)).await;
        }
    });
}
