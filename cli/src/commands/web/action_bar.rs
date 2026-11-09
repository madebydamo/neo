use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::git::dirty_state;
use super::ops::kind::id_timestamp;
use super::ops::store::{gc_old_ops, running_ops};
use super::types::AppConfig;
use super::util::{escape_html, op_id_ok, proc};
use crate::utils::OperationKind;

/// Running op shown in the action bar (first match wins), with its button label and class.
const SHOWN_OPS: [(OperationKind, &str, &str); 3] = [
    (OperationKind::Activation, "Activating…", "btn-warning"),
    (OperationKind::Update, "Updating…", "btn-info"),
    (OperationKind::Repair, "Repairing store…", "btn-warning"),
];

/// Compact signature of action-bar state so the watcher only pushes on real changes.
fn action_bar_signature(config: &AppConfig) -> String {
    gc_old_ops();
    let busy = config.eval_busy.load(Ordering::Relaxed);
    let ops = running_ops(SHOWN_OPS.map(|(k, _, _)| k)).map(Option::unwrap_or_default);
    let d = dirty_state(config);
    let dirty = d.settings_dirty || d.worktree_dirty;
    format!("{busy}|{}|{dirty}", ops.join("|"))
}

fn progress_button(kind: OperationKind, kind_label: &str, id: &str, btn_class: &str) -> String {
    let title = kind.title();
    let spinner = r#"<span class="loading loading-spinner loading-xs"></span>"#;
    if !op_id_ok(id) {
        return format!(
            r#"<button class="btn btn-sm btn-soft {btn_class} gap-1.5" title="{title} in progress">{spinner}<span class="hidden sm:inline">{kind_label}</span></button>"#,
        );
    }
    let esc = escape_html(id);
    let ts = escape_html(id_timestamp(id));
    // Title + fine timestamp; monitor response also OOBs #changes-modal-title.
    format!(
        "<button class=\"btn btn-sm btn-soft {btn_class} gap-1.5\" title=\"{title} in progress — view output\" onclick=\"var m=document.getElementById('changes-modal');var t=m.querySelector('#changes-modal-title')||m.querySelector('h3');t.innerHTML='{title} <span class=\\'font-normal text-sm opacity-50\\'>{ts}</span>';m.showModal();htmx.ajax('GET','/op/monitor/{esc}',{{target:'#changes-body',swap:'innerHTML'}})\">{spinner}<span class=\"hidden sm:inline\">{kind_label}</span></button>",
    )
}

/// Inner HTML of `#action-bar-dynamic` (status pill + reset).
/// Eval busy is exposed as `data-eval-busy` on the wrapper so the client can fold it
/// into the single navbar spinner (`#nav-busy`) with page-load busy.
/// Uses a single dirty_state pass for pending + reset.
fn render_action_bar_dynamic_inner(config: &AppConfig) -> String {
    let d = dirty_state(config);
    let running = SHOWN_OPS
        .iter()
        .zip(running_ops(SHOWN_OPS.map(|(k, _, _)| k)))
        .find_map(|(&(kind, label, class), id)| {
            id.map(|id| progress_button(kind, label, &id, class))
        });
    let pending = if let Some(button) = running {
        button
    } else if d.worktree_dirty || d.settings_dirty {
        r#"<button class="btn btn-sm btn-soft btn-warning gap-1.5" title="Saved changes not yet activated — review" onclick="var m=document.getElementById('changes-modal');m.querySelector('h3').textContent='Pending changes';m.showModal();htmx.ajax('GET','/changes/summary',{target:'#changes-body',swap:'innerHTML'})"><span class="neo-unit-dot" data-state="partial" aria-hidden="true"></span>Changes<span class="hidden lg:inline">pending</span></button>"#.to_string()
    } else {
        r#"<span class="hidden sm:inline-flex items-center gap-1.5 px-2 text-xs text-base-content/55" title="Configuration matches the last activation"><span class="neo-unit-dot" data-state="ok" aria-hidden="true"></span>Up to date</span>"#.to_string()
    };
    let reset = if d.settings_dirty || d.worktree_dirty {
        r##"<button hx-post="/actions/reset" hx-target="#changes-body" hx-swap="innerHTML" hx-confirm="Discard all uncommitted changes and return to the last committed configuration?" hx-on::after-request="if(!event.detail.successful)return;var m=document.getElementById('changes-modal');if(m){m.querySelector('h3').textContent='Discard changes';m.showModal();}" class="btn btn-sm btn-ghost btn-square" title="Discard pending changes (git restore to HEAD)" aria-label="Discard pending changes"><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4" aria-hidden="true"><path d="M9 14L4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 010 11H11"/></svg></button>"##.to_string()
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
        r#"<div id="action-bar-dynamic" class="flex items-center gap-1" data-eval-busy="{}"{oob_attr}>{}</div>"#,
        if busy { "true" } else { "false" },
        render_action_bar_dynamic_inner(config),
    )
}

/// Full OOB fragment for the action bar middle section (htmx ws extension applies it).
pub fn action_bar_oob_fragment(config: &AppConfig) -> String {
    action_bar_dynamic_element(config, true)
}

/// [`action_bar_oob_fragment`] off the async workers (it reads git state).
pub async fn action_bar_oob_async(config: &Arc<AppConfig>) -> String {
    let cfg = Arc::clone(config);
    proc::blocking(move || action_bar_oob_fragment(&cfg))
        .await
        .unwrap_or_default()
}

/// Background task: detect action-bar state changes and push OOB HTML to WS clients.
pub fn start_action_bar_watcher(config: Arc<AppConfig>) {
    tokio::spawn(async move {
        let mut last = String::new();
        loop {
            // Git + activation GC run in spawn_blocking so the async runtime stays responsive.
            let cfg = Arc::clone(&config);
            let sig = proc::blocking(move || action_bar_signature(&cfg))
                .await
                .unwrap_or_default();
            if sig != last {
                last = sig;
                config.broadcast(action_bar_oob_async(&config).await);
            }
            tokio::time::sleep(Duration::from_millis(750)).await;
        }
    });
}
