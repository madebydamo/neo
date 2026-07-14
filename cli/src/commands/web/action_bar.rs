use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::activation;
use super::git_ops::dirty_state;
use super::structs::AppConfig;
use super::util::{activation_id_ok, escape_html};

/// Compact signature of action-bar state so the watcher only pushes on real changes.
fn action_bar_signature(config: &AppConfig) -> String {
    activation::gc_old_activations();
    let busy = config.eval_busy.load(Ordering::Relaxed);
    let act = activation::find_recent_in_progress_activation().unwrap_or_default();
    let upd = activation::find_recent_in_progress_update().unwrap_or_default();
    let d = dirty_state(config);
    let dirty = d.settings_dirty || d.worktree_dirty;
    format!("{busy}|{act}|{upd}|{dirty}")
}

pub fn render_nix_busy_html(config: &AppConfig) -> String {
    if config.eval_busy.load(Ordering::Relaxed) {
        r#"<span class="inline-flex items-center gap-1 text-[10px] text-info opacity-90" title="Nix evaluator working"><span class="loading loading-spinner loading-xs"></span><span class="hidden sm:inline">eval</span></span>"#.to_string()
    } else {
        String::new()
    }
}

fn progress_button(kind_label: &str, title: &str, path_prefix: &str, id: &str, btn_class: &str) -> String {
    if !activation_id_ok(id) {
        return format!(
            r#"<button class="btn {} btn-xs animate-pulse">{} — view</button>"#,
            btn_class, kind_label
        );
    }
    let esc = escape_html(id);
    format!(
        "<button class=\"btn {btn_class} btn-xs animate-pulse\" onclick=\"var m=document.getElementById('changes-modal');m.querySelector('h3').textContent='{title}';m.showModal();htmx.ajax('GET','{path_prefix}/{esc}',{{target:'#changes-body',swap:'innerHTML'}})\">{kind_label} — view</button>",
        btn_class = btn_class,
        title = title,
        path_prefix = path_prefix,
        esc = esc,
        kind_label = kind_label,
    )
}

pub fn render_pending_changes_html(config: &AppConfig) -> String {
    if let Some(id) = activation::find_recent_in_progress_activation() {
        return progress_button(
            "Activation",
            "Activation progress",
            "/activation/monitor",
            &id,
            "btn-warning",
        );
    }
    if let Some(id) = activation::find_recent_in_progress_update() {
        return progress_button(
            "Update",
            "Update progress",
            "/update/monitor",
            &id,
            "btn-info",
        );
    }
    let d = dirty_state(config);
    if d.worktree_dirty || d.settings_dirty {
        "<button class=\"btn btn-warning btn-xs\" onclick=\"var m=document.getElementById('changes-modal');m.querySelector('h3').textContent='Pending changes';m.showModal();htmx.ajax('GET','/changes/summary',{target:'#changes-body',swap:'innerHTML'})\">Changes — review</button>".to_string()
    } else {
        "<span class=\"text-[10px] opacity-40\">clean</span>".to_string()
    }
}

pub fn render_reset_button_html(config: &AppConfig) -> String {
    let d = dirty_state(config);
    if !(d.settings_dirty || d.worktree_dirty) {
        return String::new();
    }
    // After-request only opens the modal; action-bar refresh is pushed over WS.
    // Use r## so embedded "#id" attributes do not terminate the raw string.
    r##"<button hx-post="/actions/reset" hx-target="#changes-body" hx-swap="innerHTML" hx-confirm="Reset settings from last applied (/etc/neo)?" hx-on::after-request="var m=document.getElementById('changes-modal');if(m){m.querySelector('h3').textContent='Reset';m.showModal();}" class="btn btn-xs btn-ghost">↩<span class="hidden sm:inline ml-1">Reset</span></button>"##.to_string()
}

/// Inner HTML of `#action-bar-dynamic` (appearing middle section: busy, pending, reset).
/// Uses a single dirty_state pass for pending + reset.
pub fn render_action_bar_dynamic_inner(config: &AppConfig) -> String {
    let d = dirty_state(config);
    let pending = if let Some(id) = activation::find_recent_in_progress_activation() {
        progress_button(
            "Activation",
            "Activation progress",
            "/activation/monitor",
            &id,
            "btn-warning",
        )
    } else if let Some(id) = activation::find_recent_in_progress_update() {
        progress_button(
            "Update",
            "Update progress",
            "/update/monitor",
            &id,
            "btn-info",
        )
    } else if d.worktree_dirty || d.settings_dirty {
        "<button class=\"btn btn-warning btn-xs\" onclick=\"var m=document.getElementById('changes-modal');m.querySelector('h3').textContent='Pending changes';m.showModal();htmx.ajax('GET','/changes/summary',{target:'#changes-body',swap:'innerHTML'})\">Changes — review</button>".to_string()
    } else {
        "<span class=\"text-[10px] opacity-40\">clean</span>".to_string()
    };
    let reset = if d.settings_dirty || d.worktree_dirty {
        r##"<button hx-post="/actions/reset" hx-target="#changes-body" hx-swap="innerHTML" hx-confirm="Reset settings from last applied (/etc/neo)?" hx-on::after-request="var m=document.getElementById('changes-modal');if(m){m.querySelector('h3').textContent='Reset';m.showModal();}" class="btn btn-xs btn-ghost">↩<span class="hidden sm:inline ml-1">Reset</span></button>"##.to_string()
    } else {
        String::new()
    };
    format!(
        r#"{}{}{}"#,
        render_nix_busy_html(config),
        pending,
        reset,
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
            // Git + activation GC run in spawn_blocking so the async runtime stays responsive.
            let cfg = Arc::clone(&config);
            let sig = tokio::task::spawn_blocking(move || action_bar_signature(&cfg))
                .await
                .unwrap_or_default();
            if sig != last {
                last = sig;
                let cfg = Arc::clone(&config);
                let frag = tokio::task::spawn_blocking(move || action_bar_oob_fragment(&cfg))
                    .await
                    .unwrap_or_default();
                let _ = config.unit_updates.send(frag);
            }
            tokio::time::sleep(Duration::from_millis(750)).await;
        }
    });
}
