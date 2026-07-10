use std::process::Command;
use std::sync::Arc;

use rocket::response::content::RawHtml;
use rocket::response::stream::{Event, EventStream};
use rocket::{get, post, State};
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::process::Command as AsyncCommand;

use crate::commands::web::structs::AppConfig;
use crate::commands::web::units::{
    normalize_container_unit, perform_unit_action, render_unit_controls, run_container_pull,
    schedule_unit_refresh_burst, try_begin_pull, unit_controls_oob_fragment, unit_name_valid,
    update_out_oob,
};
use crate::commands::web::util::{escape_html, sudo_cmd};

#[get("/unit/status/<unit>")]
pub fn unit_status(unit: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    if !unit_name_valid(unit) {
        return RawHtml(
            r#"<div class="unit-controls text-[10px] text-error">invalid unit</div>"#.into(),
        );
    }
    render_unit_controls(unit, config)
}

#[get("/unit/logs/<unit>")]
pub fn unit_logs(unit: &str) -> RawHtml<String> {
    let sudo = sudo_cmd();
    let out = Command::new(&sudo)
        .args([
            "journalctl",
            "-u",
            unit,
            "--no-pager",
            "-n",
            "100",
            "-o",
            "short-iso",
        ])
        .output();
    let text = match out {
        Ok(o) => {
            let mut t = String::from_utf8_lossy(&o.stdout).to_string();
            if !o.stderr.is_empty() {
                t.push_str("\n[stderr]\n");
                t.push_str(&String::from_utf8_lossy(&o.stderr));
            }
            t
        }
        Err(e) => format!("journalctl error: {}", e),
    };
    RawHtml(format!(
        r#"<pre class="text-[10px] bg-base-300 p-1 mt-1 max-h-64 overflow-auto font-mono whitespace-pre-wrap">{}</pre>"#,
        escape_html(&text)
    ))
}

/// Shared post-action path: kick systemctl, push OOB once, then burst-refresh while it settles.
/// Buttons use hx-swap="none"; the returned OOB still updates the controls row.
fn unit_action_response(
    action: &str,
    unit: &str,
    config: &State<Arc<AppConfig>>,
) -> RawHtml<String> {
    if !unit_name_valid(unit) {
        return RawHtml(String::new());
    }
    perform_unit_action(action, unit);
    let oob = unit_controls_oob_fragment(unit, config);
    let _ = config.unit_updates.send(oob.clone());
    schedule_unit_refresh_burst(unit.to_string(), Arc::clone(config));
    RawHtml(oob)
}

#[post("/unit/restart/<unit>")]
pub fn unit_restart(unit: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    unit_action_response("restart", unit, config)
}

#[post("/unit/start/<unit>")]
pub fn unit_start(unit: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    unit_action_response("start", unit, config)
}

#[post("/unit/stop/<unit>")]
pub fn unit_stop(unit: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    unit_action_response("stop", unit, config)
}

/// Kick off an async docker pull+restart. Returns immediately with OOB status +
/// disabled ↻ button; progress and completion are pushed over `/ws/status`.
#[post("/container/update/<container>")]
pub fn container_update(container: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    if !unit_name_valid(container) {
        return RawHtml(String::new());
    }
    let (unit, cname) = normalize_container_unit(container);

    if !try_begin_pull(config, &unit) {
        let out = update_out_oob(
            &unit,
            r#"<span class="inline-flex items-center gap-1 text-info"><span class="loading loading-spinner loading-xs"></span><span>already pulling…</span></span>"#,
            "docker pull already in progress",
        );
        let ctl = unit_controls_oob_fragment(&unit, config);
        return RawHtml(format!("{out}{ctl}"));
    }

    let out = update_out_oob(
        &unit,
        r#"<span class="inline-flex items-center gap-1 text-info"><span class="loading loading-spinner loading-xs"></span><span>starting pull…</span></span>"#,
        "starting docker pull",
    );
    let ctl = unit_controls_oob_fragment(&unit, config);
    let _ = config.unit_updates.send(out.clone());
    let _ = config.unit_updates.send(ctl.clone());

    let cfg = Arc::clone(config);
    tokio::spawn(async move {
        run_container_pull(unit, cname, cfg).await;
    });

    RawHtml(format!("{out}{ctl}"))
}

/// SSE endpoint for live journalctl follow in the logs dialog.
/// Client uses native EventSource; first ~100 lines + subsequent live appends.
#[get("/sse/logs/<unit>")]
pub async fn sse_logs(unit: &str) -> EventStream![] {
    let unit = unit.to_string();
    EventStream! {
        let valid = unit.chars().all(|c| c.is_alphanumeric() || "-@._".contains(c));
        if !valid {
            yield Event::data("invalid unit name for logs");
        }
        if valid {
            let sudo = sudo_cmd();
            let spawn_res = AsyncCommand::new(&sudo)
                .args([
                    "journalctl",
                    "-u",
                    &unit,
                    "-n",
                    "100",
                    "-f",
                    "--no-pager",
                    "-o",
                    "short-iso",
                ])
                .kill_on_drop(true)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();
            let mut child_opt = match spawn_res {
                Ok(c) => Some(c),
                Err(e) => {
                    yield Event::data(format!("spawn error: {}", e));
                    None
                }
            };
            if let Some(mut child) = child_opt {
                let stdout = child.stdout.take().expect("piped stdout");
                let mut lines = AsyncBufReader::new(stdout).lines();

                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            yield Event::data(escape_html(&line));
                        }
                        Ok(None) => break,
                        Err(e) => {
                            yield Event::data(format!("[read err] {}", e));
                            break;
                        }
                    }
                }
                // child auto-killed by kill_on_drop on drop
            }
        }
    }
}
