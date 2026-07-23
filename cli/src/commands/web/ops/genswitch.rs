//! Generation-switch / boot oneshot monitor fragments.
use crate::commands::web::ops::store::{load_log_tail, load_state};
use crate::commands::web::util::escape_html;

/// Lightweight monitor snippet for detached generation switch/boot oneshots.
pub fn build_genswitch_monitor_fragment(id: &str, generation: u64, mode: &str) -> String {
    let safe_id = escape_html(id);
    let mode_label = if mode == "boot" {
        "Set boot default"
    } else {
        "Switch generation"
    };
    format!(
        r#"<div class="space-y-2" id="genswitch-monitor" data-genswitch-id="{id}">
  <div class="alert alert-warning text-sm">
    <span><strong>{mode_label} {gen}</strong> started as a background job.
    The web UI may restart while the system switches — this is expected.
    Wait ~30s then reload if the page disconnects.</span>
  </div>
  <div class="text-xs opacity-60 font-mono">job {safe_id}</div>
  <div id="genswitch-status"
       hx-get="/genswitch/status/{id}"
       hx-trigger="every 2s"
       hx-swap="innerHTML">
    <span class="loading loading-spinner loading-xs"></span> starting…
  </div>
  <pre class="text-[10px] font-mono overflow-auto max-h-40 bg-base-300 p-2 rounded"
       hx-get="/genswitch/log/{id}"
       hx-trigger="every 2s"
       hx-swap="innerHTML">(log…)</pre>
</div>"#,
        id = safe_id,
        gen = generation,
        mode_label = mode_label,
        safe_id = safe_id,
    )
}

pub fn build_genswitch_status_fragment(id: &str) -> String {
    let Some(st) = load_state(id) else {
        return r#"<span class="text-xs opacity-50">waiting for state…</span>"#.to_string();
    };
    let status = st
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let phase = st.get("phase").and_then(|v| v.as_str()).unwrap_or("");
    let gen = st
        .get("generation")
        .and_then(|v| v.as_u64())
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".to_string());
    let err = st.get("error").and_then(|v| v.as_str()).unwrap_or("");
    match status {
        "success" => format!(
            r#"<span class="text-success text-sm">Generation {gen} {phase} — done. Reload the page if needed.</span>"#
        ),
        "failed" => format!(
            r#"<span class="text-error text-sm">Generation {gen} failed ({phase}): {}</span>"#,
            escape_html(err)
        ),
        _ => format!(
            r#"<span class="text-warning text-sm"><span class="loading loading-spinner loading-xs"></span> gen {gen} · {phase}</span>"#
        ),
    }
}

pub fn build_genswitch_log_fragment(id: &str) -> String {
    escape_html(&load_log_tail(id, 80))
}
