use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::activation;
use super::git_ops::dirty_state;
use super::nix_repair;
use super::structs::AppConfig;
use super::util::{activation_id_ok, escape_html, repair_id_ok};

/// Compact signature of action-bar state so the watcher only pushes on real changes.
fn action_bar_signature(config: &AppConfig) -> String {
    activation::gc_old_activations();
    let busy = config.eval_busy.load(Ordering::Relaxed);
    let act = activation::find_recent_in_progress_activation().unwrap_or_default();
    let upd = activation::find_recent_in_progress_update().unwrap_or_default();
    let rep = nix_repair::find_recent_in_progress_repair().unwrap_or_default();
    let d = dirty_state(config);
    let dirty = d.settings_dirty || d.worktree_dirty;
    format!("{busy}|{act}|{upd}|{rep}|{dirty}")
}

fn progress_button(
    kind_label: &str,
    title: &str,
    path_prefix: &str,
    id: &str,
    btn_class: &str,
) -> String {
    let id_ok = activation_id_ok(id) || repair_id_ok(id);
    if !id_ok {
        return format!(
            r#"<button class="btn {} btn-xs animate-pulse">{} — view</button>"#,
            btn_class, kind_label
        );
    }
    let esc = escape_html(id);
    // Timestamp suffix after activation_/update_ (or full id for repair).
    let ts = id
        .strip_prefix("activation_")
        .or_else(|| id.strip_prefix("update_"))
        .or_else(|| id.strip_prefix("repair_"))
        .unwrap_or(id);
    let ts = escape_html(ts);
    // Title + fine timestamp; monitor response also OOBs #changes-modal-title.
    format!(
        "<button class=\"btn {btn_class} btn-xs animate-pulse\" onclick=\"var m=document.getElementById('changes-modal');var t=m.querySelector('#changes-modal-title')||m.querySelector('h3');t.innerHTML='{title} <span class=\\'font-normal text-sm opacity-50\\'>{ts}</span>';m.showModal();htmx.ajax('GET','{path_prefix}/{esc}',{{target:'#changes-body',swap:'innerHTML'}})\">{kind_label} — view</button>",
        btn_class = btn_class,
        title = title,
        ts = ts,
        path_prefix = path_prefix,
        esc = esc,
        kind_label = kind_label,
    )
}

/// Inner HTML of `#action-bar-dynamic` (pending / reset only).
/// Eval busy is exposed as `data-eval-busy` on the wrapper so the client can fold it
/// into the single navbar spinner (`#nav-busy`) with page-load busy.
/// Uses a single dirty_state pass for pending + reset.
fn render_action_bar_dynamic_inner(config: &AppConfig) -> String {
    let d = dirty_state(config);
    let pending = if let Some(id) = activation::find_recent_in_progress_activation() {
        progress_button(
            "Activation",
            "Activation",
            "/activation/monitor",
            &id,
            "btn-warning",
        )
    } else if let Some(id) = activation::find_recent_in_progress_update() {
        progress_button("Update", "Update", "/update/monitor", &id, "btn-info")
    } else if let Some(id) = nix_repair::find_recent_in_progress_repair() {
        progress_button(
            "Store repair",
            "Nix store repair",
            "/nix/repair/monitor",
            &id,
            "btn-warning",
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
    format!(r#"{}{}"#, pending, reset)
}

/// Full `#action-bar-dynamic` element (optional OOB attr for WS pushes).
pub fn action_bar_dynamic_element(config: &AppConfig, oob: bool) -> String {
    let busy = config.eval_busy.load(Ordering::Relaxed);
    let oob_attr = if oob { r#" hx-swap-oob="true""# } else { "" };
    format!(
        r#"<div id="action-bar-dynamic" class="flex items-center gap-2" data-eval-busy="{}"{oob_attr}>{}</div>"#,
        if busy { "true" } else { "false" },
        render_action_bar_dynamic_inner(config),
    )
}

/// Full OOB fragment for the action bar middle section (htmx ws extension applies it).
pub fn action_bar_oob_fragment(config: &AppConfig) -> String {
    action_bar_dynamic_element(config, true)
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
