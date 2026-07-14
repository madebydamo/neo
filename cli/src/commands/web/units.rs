use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rocket::response::content::RawHtml;
use tokio::io::{AsyncReadExt, BufReader as AsyncBufReader};
use tokio::process::Command as AsyncCommand;

use super::structs::AppConfig;
use super::util::{escape_attr, escape_html, sudo_cmd};

pub use super::util::unit_name_valid;

/// systemctl action allowed from the web UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitAction {
    Start,
    Stop,
    Restart,
}

impl UnitAction {
    pub fn as_str(self) -> &'static str {
        match self {
            UnitAction::Start => "start",
            UnitAction::Stop => "stop",
            UnitAction::Restart => "restart",
        }
    }

    /// Parse a systemctl action string; only start/stop/restart are accepted.
    pub fn parse(action: &str) -> Option<Self> {
        match action {
            "start" => Some(UnitAction::Start),
            "stop" => Some(UnitAction::Stop),
            "restart" => Some(UnitAction::Restart),
            _ => None,
        }
    }
}

pub fn is_pull_in_flight(config: &AppConfig, unit: &str) -> bool {
    config
        .pulls_in_flight
        .lock()
        .map(|s| s.contains(unit))
        .unwrap_or(false)
}

/// Mark unit as pulling. Returns false if a pull is already in flight for this unit.
pub fn try_begin_pull(config: &AppConfig, unit: &str) -> bool {
    match config.pulls_in_flight.lock() {
        Ok(mut s) => {
            if s.contains(unit) {
                false
            } else {
                s.insert(unit.to_string());
                true
            }
        }
        Err(_) => false,
    }
}

pub fn end_pull(config: &AppConfig, unit: &str) {
    if let Ok(mut s) = config.pulls_in_flight.lock() {
        s.remove(unit);
    }
}

/// Normalize systemctl is-active stdout into a state string.
fn parse_active_state_stdout(stdout: &[u8]) -> String {
    let s = String::from_utf8_lossy(stdout).trim().to_string();
    if s.is_empty() {
        "unknown".into()
    } else {
        s
    }
}

/// Query systemctl is-active for a unit (sync; used from HTTP handlers and render).
pub fn unit_active_state(unit: &str) -> String {
    let sudo = sudo_cmd();
    Command::new(&sudo)
        .args(["systemctl", "is-active", unit])
        .output()
        .map(|o| parse_active_state_stdout(&o.stdout))
        .unwrap_or_else(|_| "unknown".into())
}

pub async fn unit_active_state_async(unit: &str) -> String {
    let sudo = sudo_cmd();
    match AsyncCommand::new(&sudo)
        .args(["systemctl", "is-active", unit])
        .output()
        .await
    {
        Ok(o) => parse_active_state_stdout(&o.stdout),
        Err(_) => "unknown".into(),
    }
}

const UPDATE_OUT_CLASSES: &str =
    "update-out update-out-inline text-[10px] ml-1 flex-shrink-0 max-w-[16rem] truncate";

/// OOB fragment for the per-row pull status slot (`#update-out-{unit}`).
pub fn update_out_oob(unit: &str, inner: &str, title: &str) -> String {
    format!(
        r#"<div id="update-out-{}" class="{}" title="{}" hx-swap-oob="true">{}</div>"#,
        escape_html(unit),
        UPDATE_OUT_CLASSES,
        escape_attr(title),
        inner
    )
}

fn status_pulling(msg: &str) -> (String, String) {
    let title = msg.to_string();
    let inner = format!(
        r#"<span class="inline-flex items-center gap-1 text-info max-w-full"><span class="loading loading-spinner loading-xs flex-shrink-0"></span><span class="truncate">{}</span></span>"#,
        escape_html(msg)
    );
    (inner, title)
}

fn status_ok(msg: &str) -> (String, String) {
    let title = msg.to_string();
    let inner = format!(
        r#"<span class="text-success truncate">✓ {}</span>"#,
        escape_html(msg)
    );
    (inner, title)
}

fn status_err(msg: &str) -> (String, String) {
    let title = msg.to_string();
    let inner = format!(
        r#"<span class="text-error truncate">✗ {}</span>"#,
        escape_html(msg)
    );
    (inner, title)
}

pub fn broadcast_update_out(unit: &str, inner: &str, title: &str, config: &AppConfig) {
    let _ = config.unit_updates.send(update_out_oob(unit, inner, title));
}

/// Push an error status for a container pull, clear in-flight, and refresh controls.
fn fail_pull(unit: &str, msg: &str, config: &AppConfig) {
    let (inner, title) = status_err(msg);
    broadcast_update_out(unit, &inner, &title, config);
    end_pull(config, unit);
    broadcast_unit_update(unit, config);
}

/// Build the inner content (dot + state + buttons) for a unit controls area.
/// Used for OOB WS pushes and composed into full divs.
///
/// Buttons stay stable across transitional states so restart/stop never "vanish"
/// while systemctl --no-block is still settling (the live WS watcher re-renders
/// as soon as ActiveState changes).
pub fn render_unit_controls_content_with_state(unit: &str, active: &str, pulling: bool) -> String {
    let is_container = unit.starts_with("docker-");

    let dot_cls = match active {
        "active" => "bg-success",
        "inactive" => "bg-base-300",
        "activating" | "deactivating" | "reloading" => "bg-info animate-pulse",
        "failed" => "bg-error",
        _ => "bg-warning",
    };

    let u = escape_html(unit);
    let state_label = escape_html(active);
    // Basic JS string escape for onclick arg (single quotes in unit names are rare for units)
    let u_js = u.replace('\'', "\\'");

    let mut inner = String::new();
    inner.push_str(&format!(
        r#"<span class="inline-block w-2 h-2 rounded-full flex-shrink-0 {}" title="{}"></span>"#,
        dot_cls,
        escape_attr(unit)
    ));
    inner.push_str(&format!(
        r#"<span class="text-[10px] opacity-60 font-mono min-w-[4.5rem]" title="ActiveState">{}</span>"#,
        state_label
    ));

    let start_btn = format!(
        r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/start/{u}" hx-swap="none" title="systemctl start">▶</button>"##,
        u = u
    );
    let restart_btn = format!(
        r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/restart/{u}" hx-swap="none" title="systemctl restart">⟳</button>"##,
        u = u
    );
    let stop_btn = format!(
        r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/stop/{u}" hx-swap="none" title="systemctl stop">⏹</button>"##,
        u = u
    );

    // Stable control set: inactive/failed → start; anything running/transitional → stop+restart.
    // failed also keeps restart so a retry is one click.
    match active {
        "inactive" => {
            inner.push_str(&start_btn);
        }
        "failed" => {
            inner.push_str(&start_btn);
            inner.push_str(&restart_btn);
        }
        _ => {
            // active | activating | deactivating | reloading | unknown
            inner.push_str(&stop_btn);
            inner.push_str(&restart_btn);
        }
    }

    // logs always opens dialog (live via SSE)
    inner.push_str(&format!(
        r#"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" onclick="openUnitLogs('{}')" title="open live logs dialog (infinitely scrollable)">logs</button>"#,
        u_js
    ));

    if is_container {
        if pulling {
            inner.push_str(
                r#"<button class="btn btn-accent btn-xs h-5 min-h-0 px-1.5 btn-disabled" disabled title="docker pull in progress"><span class="loading loading-spinner loading-xs"></span></button>"#,
            );
        } else {
            // hx-swap=none: immediate OOB (update-out + controls) comes from the response;
            // long pull progress is pushed over /ws/status.
            inner.push_str(&format!(
                r##"<button class="btn btn-accent btn-xs h-5 min-h-0 px-1.5" hx-post="/container/update/{u}" hx-swap="none" hx-disabled-elt="this" title="docker pull (current running image) + restart">↻</button>"##,
                u = u
            ));
        }
    }

    inner
}

/// Full unit-controls div (with id) for bootstrap GET.
pub fn render_unit_controls(unit: &str, config: &AppConfig) -> RawHtml<String> {
    let active = unit_active_state(unit);
    let pulling = is_pull_in_flight(config, unit);
    let content = render_unit_controls_content_with_state(unit, &active, pulling);
    let u = escape_html(unit);
    RawHtml(format!(
        r#"<div id="unit-controls-{u}" class="unit-controls flex items-center gap-1 flex-shrink-0" data-active-state="{}">{content}</div>"#,
        escape_attr(&active)
    ))
}

/// OOB fragment for htmx ws (and action HTTP responses).
pub fn unit_controls_oob_fragment(unit: &str, config: &AppConfig) -> String {
    let active = unit_active_state(unit);
    let pulling = is_pull_in_flight(config, unit);
    unit_controls_oob_fragment_with_state(unit, &active, pulling)
}

pub fn unit_controls_oob_fragment_with_state(unit: &str, active: &str, pulling: bool) -> String {
    format!(
        r#"<div id="unit-controls-{}" class="unit-controls flex items-center gap-1 flex-shrink-0" data-active-state="{}" hx-swap-oob="true">{}</div>"#,
        escape_html(unit),
        escape_attr(active),
        render_unit_controls_content_with_state(unit, active, pulling)
    )
}

/// Broadcast an OOB swap fragment for a unit's controls to all connected WS clients.
pub fn broadcast_unit_update(unit: &str, config: &AppConfig) {
    let _ = config
        .unit_updates
        .send(unit_controls_oob_fragment(unit, config));
}

/// After a non-blocking systemctl action, ActiveState may lag for a few seconds.
/// Push a short burst of refreshes so the UI settles without waiting for the next
/// watcher tick alone (and even if the pane only did a one-shot HTTP OOB).
pub fn schedule_unit_refresh_burst(unit: String, config: Arc<AppConfig>) {
    if !unit_name_valid(&unit) {
        return;
    }
    tokio::spawn(async move {
        for delay_ms in [150_u64, 400, 900, 1800, 3500] {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            broadcast_unit_update(&unit, &config);
        }
    });
}

pub fn perform_unit_action(action: UnitAction, unit: &str) {
    if !unit_name_valid(unit) {
        return;
    }
    let sudo = sudo_cmd();
    let _ = Command::new(&sudo)
        .args([
            "systemctl",
            action.as_str(),
            unit,
            "--no-block",
            "--no-ask-password",
        ])
        .status();
}

/// Best-effort parse of `id="unit-controls-…"` + `data-active-state="…"` from an OOB fragment.
pub fn extract_unit_state_from_oob(fragment: &str) -> Option<(String, String)> {
    let id_marker = r#"id="unit-controls-"#;
    let state_marker = r#"data-active-state=""#;
    let id_start = fragment.find(id_marker)? + id_marker.len();
    let id_end = fragment[id_start..].find('"')? + id_start;
    let unit = fragment[id_start..id_end].to_string();
    let state_start = fragment.find(state_marker)? + state_marker.len();
    let state_end = fragment[state_start..].find('"')? + state_start;
    let state = fragment[state_start..state_end].to_string();
    if unit_name_valid(&unit) {
        Some((unit, state))
    } else {
        None
    }
}

/// Normalize path param to (systemd unit name, bare docker container name).
pub fn normalize_container_unit(container: &str) -> (String, String) {
    if container.starts_with("docker-") {
        let bare = container[7..].to_string();
        (container.to_string(), bare)
    } else {
        (format!("docker-{}", container), container.to_string())
    }
}

fn last_progress_line(buf: &str) -> Option<&str> {
    buf.split(|c| c == '\r' || c == '\n')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .last()
}

fn spawn_pull_pipe_reader(
    pipe: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    tx: tokio::sync::mpsc::Sender<String>,
) {
    tokio::spawn(async move {
        let mut reader = AsyncBufReader::new(pipe);
        let mut buf = [0u8; 512];
        let mut acc = String::new();
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if acc.len() > 4096 {
                        acc = acc[acc.len() - 2048..].to_string();
                    }
                    if let Some(line) = last_progress_line(&acc) {
                        let _ = tx.try_send(line.to_string());
                    }
                }
            }
        }
    });
}

/// Background: docker inspect → pull (stream progress over WS) → systemctl restart.
pub async fn run_container_pull(unit: String, cname: String, config: Arc<AppConfig>) {
    let push = |inner: String, title: String| {
        broadcast_update_out(&unit, &inner, &title, &config);
    };

    let inspect = AsyncCommand::new("docker")
        .args(["inspect", "--format", "{{.Config.Image}}", &cname])
        .output()
        .await;

    let img = match inspect {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                fail_pull(&unit, "no image from inspect", &config);
                return;
            }
            s
        }
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            fail_pull(&unit, &format!("inspect failed: {}", err.trim()), &config);
            return;
        }
        Err(e) => {
            fail_pull(&unit, &format!("docker error: {}", e), &config);
            return;
        }
    };

    {
        let (inner, title) = status_pulling(&format!("pulling {}", img));
        push(inner, title);
    }

    let mut child = match AsyncCommand::new("docker")
        .args(["pull", &img])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            fail_pull(&unit, &format!("pull spawn: {}", e), &config);
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<String>(16);

    if let Some(pipe) = stdout {
        spawn_pull_pipe_reader(pipe, line_tx.clone());
    }
    if let Some(pipe) = stderr {
        spawn_pull_pipe_reader(pipe, line_tx);
    }

    let unit_p = unit.clone();
    let config_p = Arc::clone(&config);
    let img_p = img.clone();
    let progress_task = tokio::spawn(async move {
        let started = Instant::now();
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut last_line = format!("pulling {}", img_p);
        let mut ticker = tokio::time::interval(Duration::from_millis(500));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                msg = line_rx.recv() => {
                    match msg {
                        Some(line) => {
                            last_line = if line.len() > 80 {
                                format!("{}…", &line[..77])
                            } else {
                                line
                            };
                            if last_emit.elapsed() >= Duration::from_millis(350) {
                                let (inner, title) = status_pulling(&last_line);
                                broadcast_update_out(&unit_p, &inner, &title, &config_p);
                                last_emit = Instant::now();
                            }
                        }
                        None => break,
                    }
                }
                _ = ticker.tick() => {
                    // Heartbeat so a quiet pull still shows the UI is alive.
                    if last_emit.elapsed() >= Duration::from_secs(2) {
                        let secs = started.elapsed().as_secs();
                        let msg = format!("pulling {} ({}s)", img_p, secs);
                        let (inner, title) = status_pulling(&msg);
                        broadcast_update_out(&unit_p, &inner, &title, &config_p);
                        last_emit = Instant::now();
                    }
                }
            }
        }
    });

    let status = child.wait().await;
    // Drop senders finished with pipes; wait for progress UI task.
    let _ = progress_task.await;

    match status {
        Ok(s) if s.success() => {
            let (inner, title) = status_pulling("restarting…");
            push(inner, title);

            let sudo = sudo_cmd();
            let _ = AsyncCommand::new(&sudo)
                .args([
                    "systemctl",
                    "restart",
                    &unit,
                    "--no-block",
                    "--no-ask-password",
                ])
                .status()
                .await;

            let (inner, title) = status_ok(&format!("updated {}", img));
            push(inner, title);
            end_pull(&config, &unit);
            broadcast_unit_update(&unit, &config);
            schedule_unit_refresh_burst(unit, config);
        }
        Ok(s) => {
            fail_pull(
                &unit,
                &format!("pull exit {}", s.code().unwrap_or(-1)),
                &config,
            );
        }
        Err(e) => {
            fail_pull(&unit, &format!("pull wait: {}", e), &config);
        }
    }
}
