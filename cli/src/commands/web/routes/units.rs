use std::process::Command;
use std::sync::Arc;

use rocket::response::content::RawHtml;
use rocket::response::stream::{Event, EventStream};
use rocket::{get, post, State};
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::process::Command as AsyncCommand;

use crate::commands::web::structs::AppConfig;
use crate::commands::web::units::{
    broadcast_unit_update, perform_unit_action, render_unit_controls, schedule_unit_refresh_burst,
    unit_controls_oob_fragment, unit_name_valid,
};
use crate::commands::web::util::{escape_html, sudo_cmd};

#[get("/unit/status/<unit>")]
pub fn unit_status(unit: &str) -> RawHtml<String> {
    if !unit_name_valid(unit) {
        return RawHtml(
            r#"<div class="unit-controls text-[10px] text-error">invalid unit</div>"#.into(),
        );
    }
    render_unit_controls(unit)
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
fn unit_action_response(action: &str, unit: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    if !unit_name_valid(unit) {
        return RawHtml(String::new());
    }
    perform_unit_action(action, unit);
    let oob = unit_controls_oob_fragment(unit);
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

#[post("/container/update/<container>")]
pub fn container_update(container: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    // Normalize: accept "foo" or "docker-foo"; use bare name for inspect/restart
    let cname = if container.starts_with("docker-") {
        &container[7..]
    } else {
        container
    };
    // Inspect current image ref from the running container (works for :latest and pinned)
    let inspect = Command::new("docker")
        .args(["inspect", "--format", "{{.Config.Image}}", cname])
        .output();
    let img = match inspect {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Ok(o) => {
            return RawHtml(format!(
                r#"<span class="text-error text-xs">inspect failed: {}</span>"#,
                escape_html(&String::from_utf8_lossy(&o.stderr))
            ))
        }
        Err(e) => {
            return RawHtml(format!(
                r#"<span class="text-error text-xs">docker error: {}</span>"#,
                e
            ))
        }
    };
    if img.is_empty() {
        return RawHtml(r#"<span class="text-error text-xs">no image from inspect</span>"#.into());
    }
    let pull = Command::new("docker").args(["pull", &img]).output();
    let pull_out = match pull {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => format!("pull error: {}", e),
    };
    // Restart via sudo to let the unit manage it (use docker- prefix for unit)
    let sudo = sudo_cmd();
    let _ = Command::new(&sudo)
        .args([
            "systemctl",
            "restart",
            &format!("docker-{}", cname),
            "--no-block",
            "--no-ask-password",
        ])
        .status();
    // Live unit-control updates over WS (burst while restart settles).
    let unit_for_watch = if container.starts_with("docker-") {
        container.to_string()
    } else {
        format!("docker-{}", cname)
    };
    broadcast_unit_update(&unit_for_watch, &config);
    schedule_unit_refresh_burst(unit_for_watch, Arc::clone(config));
    RawHtml(format!(
        r#"<div class="text-xs"><div>pulled: {}</div><pre class="text-[9px] max-h-32 overflow-auto">{}</pre><div class="text-success">restarted docker-{}</div></div>"#,
        escape_html(&img),
        escape_html(&pull_out),
        escape_html(cname)
    ))
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
