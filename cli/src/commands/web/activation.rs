use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::log;

pub fn activation_dir() -> PathBuf {
    log::operations_dir()
}

pub fn load_activation_state(id: &str) -> Option<serde_json::Value> {
    let p = activation_dir().join(format!("{}.json", id));
    if let Ok(s) = fs::read_to_string(&p) {
        if let Ok(v) = serde_json::from_str(&s) {
            return Some(v);
        }
    }
    None
}

pub fn load_log_tail(id: &str, n: usize) -> String {
    let p = activation_dir().join(format!("{}.log", id));
    if let Ok(content) = fs::read_to_string(&p) {
        let lines: Vec<&str> = content.lines().collect();
        let start = if lines.len() > n { lines.len() - n } else { 0 };
        let tail = lines[start..].join("\n");
        return tail;
    }
    "(no log yet)".to_string()
}

pub fn find_recent_in_progress_activation() -> Option<String> {
    let dir = activation_dir();
    if !dir.exists() {
        return None;
    }
    let mut best: Option<(String, u64)> = None;
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".json") && name.starts_with("activation_") {
                if let Ok(meta) = e.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        let t = mtime
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        if now > t + 3600 {
                            continue;
                        }
                        if let Ok(s) = fs::read_to_string(e.path()) {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                                if v.get("status").and_then(|x| x.as_str()) == Some("in_progress") {
                                    if best.as_ref().map_or(true, |&(_, bt)| t > bt) {
                                        let id = name.trim_end_matches(".json").to_string();
                                        best = Some((id, t));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    best.map(|(id, _)| id)
}

pub fn gc_old_activations() {
    let dir = activation_dir();
    if !dir.exists() {
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if let Ok(meta) = e.metadata() {
                if let Ok(mtime) = meta.modified() {
                    let t = mtime
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    if now > t + 7 * 86400 {
                        let _ = fs::remove_file(&p);
                    }
                }
            }
        }
    }
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|_| fs::read_dir(&dir).unwrap())
        .flatten()
        .collect();
    if files.len() > 10 {
        files.sort_by_key(|e| {
            e.metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        });
        for old in files.iter().take(files.len() - 10) {
            let _ = fs::remove_file(old.path());
        }
    }
}

pub fn build_monitor_fragment(id: &str) -> String {
    gc_old_activations();
    let st = load_activation_state(id);
    let status = st
        .as_ref()
        .and_then(|v| v.get("status").and_then(|s| s.as_str()))
        .unwrap_or("unknown");
    let phase = st
        .as_ref()
        .and_then(|v| v.get("phase").and_then(|s| s.as_str()))
        .unwrap_or("");
    let branch = st
        .as_ref()
        .and_then(|v| v.get("branch").and_then(|s| s.as_str()))
        .unwrap_or("");
    let err = st
        .as_ref()
        .and_then(|v| v.get("error").and_then(|s| s.as_str()))
        .unwrap_or("");
    let log_url = format!("/activation/log/{}", id);
    let status_url = format!("/activation/status/{}", id);
    let mut html = String::new();
    html.push_str(&format!(
        r#"<div id="activation-monitor" data-id="{}" class="p-2 bg-base-200 rounded">
<div class="text-sm font-semibold">Activation {} — phase: {}</div>"#,
        id, id, phase
    ));
    if status == "in_progress" {
        html.push_str(r#"<div class="text-warning text-xs">Running (UI may restart during rebuild). Logs update live.</div>"#);
    } else if status == "success" {
        html.push_str(&format!(
            r#"<div class="alert alert-success text-sm">Success as {}</div>"#,
            branch
        ));
    } else if status == "failed" {
        html.push_str(&format!(
            r#"<div class="alert alert-error text-sm">Failed: {}</div>"#,
            err
        ));
    }
    html.push_str(&format!(
        r#"<div id="act-log" class="text-[10px] bg-base-300 p-1 mt-1 max-h-80 overflow-auto font-mono" hx-get="{}" hx-trigger="load, every 1s" hx-swap="innerHTML"></div>"#,
        log_url
    ));
    html.push_str(&format!(
        r#"<div id="act-status" hx-get="{}" hx-trigger="load, every 5s" hx-swap="outerHTML" class="text-xs mt-1"></div>"#,
        status_url
    ));
    if status == "success" {
        html.push_str(r#"<div class="mt-2"><button onclick="var d=document.getElementById('activation-success');var b=document.getElementById('activation-success-body');b.innerHTML=this.closest('#activation-monitor').innerHTML;d.showModal();localStorage.removeItem('neo.pendingActivation');document.getElementById('changes-modal').close();" class="btn btn-sm btn-success">Confirm & reload</button></div>"#);
    }
    html.push_str("</div>");
    html
}

pub fn build_status_fragment(id: &str) -> String {
    let st = load_activation_state(id);
    let status = st
        .as_ref()
        .and_then(|v| v.get("status").and_then(|s| s.as_str()))
        .unwrap_or("unknown");
    let phase = st
        .as_ref()
        .and_then(|v| v.get("phase").and_then(|s| s.as_str()))
        .unwrap_or("");
    let branch = st
        .as_ref()
        .and_then(|v| v.get("branch").and_then(|s| s.as_str()))
        .unwrap_or(id);
    if status == "success" {
        format!(
            r#"<div class="text-success text-xs">complete: {}</div>"#,
            branch
        )
    } else if status == "failed" {
        format!(
            r#"<div class="text-error text-xs">failed in {}</div>"#,
            phase
        )
    } else if status == "in_progress" {
        format!(
            r#"<div class="text-warning text-xs">in progress: {}</div>"#,
            phase
        )
    } else {
        format!(
            r#"<div class="text-xs opacity-60">status: {}</div>"#,
            status
        )
    }
}

pub fn build_log_fragment(id: &str) -> String {
    let tail = load_log_tail(id, 300);
    let esc = tail
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!("<pre class=\"whitespace-pre-wrap\">{}</pre>", esc)
}

pub fn is_activation_in_progress() -> bool {
    find_recent_in_progress_activation().is_some()
}

pub fn find_recent_in_progress_update() -> Option<String> {
    let dir = activation_dir();
    if !dir.exists() {
        return None;
    }
    let mut best: Option<(String, u64)> = None;
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".json") && name.starts_with("update_") {
                if let Ok(meta) = e.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        let t = mtime
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        if now > t + 3600 {
                            continue;
                        }
                        if let Ok(s) = fs::read_to_string(e.path()) {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                                if v.get("status").and_then(|x| x.as_str()) == Some("in_progress") {
                                    if best.as_ref().map_or(true, |&(_, bt)| t > bt) {
                                        let id = name.trim_end_matches(".json").to_string();
                                        best = Some((id, t));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    best.map(|(id, _)| id)
}

pub fn build_update_monitor_fragment(id: &str) -> String {
    gc_old_activations();
    let st = load_activation_state(id);
    let status = st
        .as_ref()
        .and_then(|v| v.get("status").and_then(|s| s.as_str()))
        .unwrap_or("unknown");
    let phase = st
        .as_ref()
        .and_then(|v| v.get("phase").and_then(|s| s.as_str()))
        .unwrap_or("");
    let err = st
        .as_ref()
        .and_then(|v| v.get("error").and_then(|s| s.as_str()))
        .unwrap_or("");
    let log_url = format!("/update/log/{}", id);
    let status_url = format!("/update/status/{}", id);
    let mut html = String::new();
    html.push_str(&format!(
        r#"<div id="update-monitor" data-id="{}" class="p-2 bg-base-200 rounded">
<div class="text-sm font-semibold">Update {} — phase: {}</div>"#,
        id, id, phase
    ));
    if status == "in_progress" {
        html.push_str(r#"<div class="text-info text-xs">Running. Logs update live.</div>"#);
    } else if status == "success" {
        html.push_str(r#"<div class="alert alert-success text-sm">Update complete</div>"#);
    } else if status == "failed" {
        html.push_str(&format!(
            r#"<div class="alert alert-error text-sm">Failed: {}</div>"#,
            err
        ));
    }
    html.push_str(&format!(
        r#"<div id="update-log" class="text-[10px] bg-base-300 p-1 mt-1 max-h-80 overflow-auto font-mono" hx-get="{}" hx-trigger="load, every 1s" hx-swap="innerHTML"></div>"#,
        log_url
    ));
    html.push_str(&format!(
        r#"<div id="update-status" hx-get="{}" hx-trigger="load, every 5s" hx-swap="outerHTML" class="text-xs mt-1"></div>"#,
        status_url
    ));
    html.push_str("</div>");
    html
}

pub fn build_update_status_fragment(id: &str) -> String {
    let st = load_activation_state(id);
    let status = st
        .as_ref()
        .and_then(|v| v.get("status").and_then(|s| s.as_str()))
        .unwrap_or("unknown");
    let phase = st
        .as_ref()
        .and_then(|v| v.get("phase").and_then(|s| s.as_str()))
        .unwrap_or("");
    if status == "success" {
        format!(r#"<div class="text-success text-xs">update complete</div>"#)
    } else if status == "failed" {
        format!(
            r#"<div class="text-error text-xs">update failed in {}</div>"#,
            phase
        )
    } else if status == "in_progress" {
        format!(r#"<div class="text-info text-xs">update: {}</div>"#, phase)
    } else {
        format!(
            r#"<div class="text-xs opacity-60">status: {}</div>"#,
            status
        )
    }
}
