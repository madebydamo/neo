// Long-running Nix store repair jobs triggered from the web UI.
// Does not hold the evaluator mutex while `nix-store` runs; refreshes the repl afterwards.
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;

use crate::commands::log::{operations_dir, OPERATIONS_DIR};
use crate::commands::web::structs::AppConfig;
use crate::commands::web::util::{escape_html, nix_bin, repair_id_ok, sudo_cmd};

pub fn repair_dir() -> PathBuf {
    operations_dir()
}

fn state_path(id: &str) -> PathBuf {
    repair_dir().join(format!("{id}.json"))
}

fn log_path(id: &str) -> PathBuf {
    repair_dir().join(format!("{id}.log"))
}

fn write_state(id: &str, status: &str, phase: &str, err: Option<&str>) {
    let _ = fs::create_dir_all(repair_dir());
    let mut s = serde_json::json!({
        "id": id,
        "status": status,
        "phase": phase,
        "log_path": log_path(id).to_string_lossy(),
    });
    if let Some(e) = err {
        s["error"] = serde_json::json!(e);
    }
    let _ = fs::write(
        state_path(id),
        serde_json::to_string_pretty(&s).unwrap_or_else(|_| "{}".to_string()),
    );
}

pub fn load_repair_state(id: &str) -> Option<serde_json::Value> {
    if !repair_id_ok(id) {
        return None;
    }
    let p = state_path(id);
    let s = fs::read_to_string(p).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn load_repair_log_tail(id: &str, n: usize) -> String {
    if !repair_id_ok(id) {
        return "(invalid id)".to_string();
    }
    let p = log_path(id);
    if let Ok(content) = fs::read_to_string(&p) {
        let lines: Vec<&str> = content.lines().collect();
        let start = if lines.len() > n { lines.len() - n } else { 0 };
        return lines[start..].join("\n");
    }
    "(no log yet)".to_string()
}

/// Most recent in-progress repair within the last hour, if any.
pub fn find_recent_in_progress_repair() -> Option<String> {
    let dir = repair_dir();
    if !dir.exists() {
        return None;
    }
    let mut best: Option<(String, u64)> = None;
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if !(name.ends_with(".json") && name.starts_with("repair_")) {
                continue;
            }
            let Ok(meta) = e.metadata() else { continue };
            let Ok(mtime) = meta.modified() else { continue };
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
            let Ok(s) = fs::read_to_string(e.path()) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) else {
                continue;
            };
            if v.get("status").and_then(|x| x.as_str()) != Some("in_progress") {
                continue;
            }
            if best.as_ref().map_or(true, |&(_, bt)| t > bt) {
                let id = name.trim_end_matches(".json").to_string();
                best = Some((id, t));
            }
        }
    }
    best.map(|(id, _)| id)
}

/// Start a background store verify+repair. Returns the operation id.
/// Single-flight: if one is already running, returns that id instead of starting another.
pub fn start_store_verify_repair(config: Arc<AppConfig>) -> String {
    if let Some(existing) = find_recent_in_progress_repair() {
        return existing;
    }
    let ts = crate::commands::get_timestamp();
    let id = format!("repair_{ts}");
    let _ = fs::create_dir_all(OPERATIONS_DIR);
    write_state(&id, "in_progress", "starting", None);
    let _ = fs::write(
        log_path(&id),
        format!("{id} store verify/repair triggered via web at {ts}\n"),
    );

    let id_for_task = id.clone();
    tokio::spawn(async move {
        run_store_verify_repair(config, id_for_task).await;
    });
    id
}

async fn run_store_verify_repair(config: Arc<AppConfig>, id: String) {
    write_state(&id, "in_progress", "nix-store-verify-repair", None);
    let log_file = log_path(&id);
    let sudo = sudo_cmd();
    // Prefer the same nix binary neo-web uses when available.
    let nix_store = {
        let nix = nix_bin();
        let p = std::path::Path::new(&nix);
        if let Some(parent) = p.parent() {
            let candidate = parent.join("nix-store");
            if candidate.is_file() {
                candidate.to_string_lossy().into_owned()
            } else {
                "nix-store".to_string()
            }
        } else {
            "nix-store".to_string()
        }
    };

    // Use sudo -n (non-interactive). Do not pass --no-ask-password: that is a
    // systemctl flag, not a sudo option. Skip --check-contents (full content
    // rehash is very slow); path existence + repair is enough for missing store paths.
    let mut child = match TokioCommand::new(&sudo)
        .args(["-n", &nix_store, "--verify", "--repair"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("failed to spawn store repair: {e}");
            append_log(&log_file, &msg);
            write_state(&id, "failed", "spawn", Some(&msg));
            return;
        }
    };

    // Stream stdout+stderr into the log file.
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let log_out = log_file.clone();
    let log_err = log_file.clone();
    let out_task = tokio::spawn(async move {
        if let Some(out) = stdout {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                append_log(&log_out, &line);
            }
        }
    });
    let err_task = tokio::spawn(async move {
        if let Some(err) = stderr {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                append_log(&log_err, &line);
            }
        }
    });

    let status = child.wait().await;
    let _ = out_task.await;
    let _ = err_task.await;

    match status {
        Ok(st) if st.success() => {
            append_log(&log_file, "nix-store --verify --repair completed OK");
            write_state(&id, "in_progress", "refresh-repl", None);
            // Restart the persistent repl and warm extracts so the UI can recover.
            {
                let mut ev = config.evaluator.lock().await;
                match ev.refresh().await {
                    Ok(()) => {
                        append_log(&log_file, "nix repl refreshed after store repair");
                        let nav = ev.extract_proxied_services().await;
                        if let Some(err) = nav.error.as_ref() {
                            append_log(&log_file, &format!("warm-up still reports error: {err}"));
                        } else {
                            append_log(&log_file, "warm-up navigator extract succeeded");
                        }
                        let _ = ev.extract_neo_theme().await;
                    }
                    Err(e) => {
                        append_log(&log_file, &format!("repl refresh failed: {e:#}"));
                        write_state(
                            &id,
                            "failed",
                            "refresh-repl",
                            Some(&format!("store repair OK but repl refresh failed: {e:#}")),
                        );
                        return;
                    }
                }
            }
            {
                let mut cache = config.schema_cache.write().await;
                cache.invalidate_all();
            }
            write_state(&id, "success", "complete", None);
            append_log(&log_file, "repair job complete");
        }
        Ok(st) => {
            let msg = format!("nix-store repair exited with status {st}");
            append_log(&log_file, &msg);
            write_state(&id, "failed", "nix-store-verify-repair", Some(&msg));
        }
        Err(e) => {
            let msg = format!("wait on nix-store failed: {e}");
            append_log(&log_file, &msg);
            write_state(&id, "failed", "nix-store-verify-repair", Some(&msg));
        }
    }
}

fn append_log(path: &std::path::Path, line: &str) {
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

pub fn build_repair_monitor_fragment(id: &str) -> String {
    if !repair_id_ok(id) {
        return format!(
            r#"<div class="alert alert-error text-sm">invalid repair id: {}</div>"#,
            escape_html(id)
        );
    }
    let st = load_repair_state(id);
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

    let mut html = String::new();
    html.push_str(&format!(
        r#"<div id="repair-monitor" data-id="{}" class="p-2 bg-base-200 rounded">
<div class="text-sm font-semibold">Nix store repair {}</div>"#,
        escape_html(id),
        escape_html(id),
    ));
    match status {
        "in_progress" => {
            html.push_str(&format!(
                r#"<div class="text-warning text-xs">Running ({}) — can take several minutes. Logs update live.</div>"#,
                escape_html(phase)
            ));
        }
        "success" => {
            html.push_str(
                r#"<div class="alert alert-success text-sm">Store repair finished. Reload the page to re-evaluate.</div>
<div class="mt-2"><button type="button" class="btn btn-sm btn-success" onclick="location.reload()">Reload</button></div>"#,
            );
        }
        "failed" => {
            html.push_str(&format!(
                r#"<div class="alert alert-error text-sm">Failed: {}</div>"#,
                escape_html(err)
            ));
        }
        _ => {
            html.push_str(&format!(
                r#"<div class="text-xs opacity-60">status: {}</div>"#,
                escape_html(status)
            ));
        }
    }
    let hx = if status == "in_progress" {
        format!(
            r#" hx-get="/nix/repair/status/{}" hx-trigger="load, every 1s" hx-swap="outerHTML""#,
            escape_html(id)
        )
    } else {
        String::new()
    };
    html.push_str(&format!(
        r#"<div id="repair-status"{hx} class="text-xs mt-1">{phase_line}</div>"#,
        hx = hx,
        phase_line = format!(
            r#"<span class="{}">{}</span>"#,
            if status == "success" {
                "text-success"
            } else if status == "failed" {
                "text-error"
            } else {
                "text-warning"
            },
            escape_html(&format!("{status}: {phase}"))
        ),
    ));
    html.push_str(&format!(
        r#"<div id="repair-log" class="text-[10px] bg-base-300 p-1 mt-1 max-h-80 overflow-auto font-mono" hx-get="/nix/repair/log/{}" hx-trigger="load, every 1s" hx-swap="innerHTML"></div>"#,
        escape_html(id)
    ));
    html.push_str("</div>");
    html
}

pub fn build_repair_status_fragment(id: &str) -> String {
    if !repair_id_ok(id) {
        return format!(
            r#"<div class="alert alert-error text-sm">invalid repair id: {}</div>"#,
            escape_html(id)
        );
    }
    let st = load_repair_state(id);
    let status = st
        .as_ref()
        .and_then(|v| v.get("status").and_then(|s| s.as_str()))
        .unwrap_or("unknown");
    let phase = st
        .as_ref()
        .and_then(|v| v.get("phase").and_then(|s| s.as_str()))
        .unwrap_or("");
    let hx = if status == "in_progress" {
        format!(
            r#" hx-get="/nix/repair/status/{}" hx-trigger="load, every 1s" hx-swap="outerHTML""#,
            escape_html(id)
        )
    } else {
        String::new()
    };
    let cls = match status {
        "success" => "text-success",
        "failed" => "text-error",
        "in_progress" => "text-warning",
        _ => "opacity-60",
    };
    format!(
        r#"<div id="repair-status"{hx} class="text-xs mt-1"><span class="{cls}">{label}</span></div>"#,
        hx = hx,
        cls = cls,
        label = escape_html(&format!("{status}: {phase}")),
    )
}

pub fn build_repair_log_fragment(id: &str) -> String {
    if !repair_id_ok(id) {
        return format!(
            r#"<div class="alert alert-error text-sm">invalid repair id: {}</div>"#,
            escape_html(id)
        );
    }
    let tail = load_repair_log_tail(id, 300);
    format!(
        "<pre class=\"whitespace-pre-wrap\">{}</pre>",
        escape_html(&tail)
    )
}
