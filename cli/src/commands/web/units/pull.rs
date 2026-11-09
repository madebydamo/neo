//! Container image pull + restart background job.
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, BufReader as AsyncBufReader};
use tokio::process::Command as AsyncCommand;

use super::super::types::AppConfig;
use super::super::util::{docker_bin, status_err, status_ok, status_pulling};
use super::control::{
    broadcast_unit_update, broadcast_update_out, end_pull, perform_unit_action,
    schedule_unit_refresh_burst, UnitAction,
};

/// Push an error status for a container pull, clear in-flight, and refresh controls.
async fn fail_pull(unit: &str, msg: &str, config: &AppConfig) {
    let (inner, title) = status_err(msg);
    broadcast_update_out(unit, &inner, &title, config);
    end_pull(config, unit);
    broadcast_unit_update(unit, config).await;
}

fn last_progress_line(buf: &str) -> Option<&str> {
    buf.split(['\r', '\n'])
        .map(str::trim)
        .rfind(|s| !s.is_empty())
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

/// The image a container runs; `Err` is the status message.
async fn inspect_image(docker: &str, cname: &str) -> Result<String, String> {
    let o = AsyncCommand::new(docker)
        .args(["inspect", "--format", "{{.Config.Image}}", cname])
        .output()
        .await
        .map_err(|e| format!("docker error: {e}"))?;
    if !o.status.success() {
        let err = String::from_utf8_lossy(&o.stderr);
        return Err(format!("inspect failed: {}", err.trim()));
    }
    let img = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if img.is_empty() {
        return Err("no image from inspect".to_string());
    }
    Ok(img)
}

/// `docker pull`, streaming progress to the unit's status slot; `Err` is the status message.
async fn pull_image(
    docker: &str,
    img: &str,
    unit: &str,
    config: &Arc<AppConfig>,
) -> Result<(), String> {
    let mut child = AsyncCommand::new(docker)
        .args(["pull", img])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("pull spawn: {e}"))?;

    let (line_tx, line_rx) = tokio::sync::mpsc::channel::<String>(16);
    if let Some(pipe) = child.stdout.take() {
        spawn_pull_pipe_reader(pipe, line_tx.clone());
    }
    if let Some(pipe) = child.stderr.take() {
        spawn_pull_pipe_reader(pipe, line_tx);
    }
    let progress_task = tokio::spawn(report_pull_progress(
        line_rx,
        unit.to_string(),
        img.to_string(),
        Arc::clone(config),
    ));

    let status = child.wait().await;
    // Drop senders finished with pipes; wait for progress UI task.
    let _ = progress_task.await;
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("pull exit {}", s.code().unwrap_or(-1))),
        Err(e) => Err(format!("pull wait: {e}")),
    }
}

/// Throttled progress lines, with a heartbeat so a quiet pull still shows the UI is alive.
async fn report_pull_progress(
    mut lines: tokio::sync::mpsc::Receiver<String>,
    unit: String,
    img: String,
    config: Arc<AppConfig>,
) {
    let started = Instant::now();
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let mut ticker = tokio::time::interval(Duration::from_millis(500));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            msg = lines.recv() => {
                let Some(line) = msg else { break };
                let line = if line.len() > 80 {
                    format!("{}…", &line[..77])
                } else {
                    line
                };
                if last_emit.elapsed() >= Duration::from_millis(350) {
                    let (inner, title) = status_pulling(&line);
                    broadcast_update_out(&unit, &inner, &title, &config);
                    last_emit = Instant::now();
                }
            }
            _ = ticker.tick() => {
                if last_emit.elapsed() >= Duration::from_secs(2) {
                    let secs = started.elapsed().as_secs();
                    let (inner, title) = status_pulling(&format!("pulling {img} ({secs}s)"));
                    broadcast_update_out(&unit, &inner, &title, &config);
                    last_emit = Instant::now();
                }
            }
        }
    }
}

/// Background: docker inspect → pull (stream progress over WS) → systemctl restart.
pub async fn run_container_pull(unit: String, cname: String, config: Arc<AppConfig>) {
    let push = |msg: &str| {
        let (inner, title) = status_pulling(msg);
        broadcast_update_out(&unit, &inner, &title, &config);
    };
    let docker = docker_bin();
    let pulled = async {
        let img = inspect_image(&docker, &cname).await?;
        push(&format!("pulling {img}"));
        pull_image(&docker, &img, &unit, &config).await?;
        push("restarting…");
        perform_unit_action(UnitAction::Restart, &unit)
            .await
            .map_err(|e| format!("pulled {img}, restart failed: {e}"))?;
        Ok::<_, String>(img)
    }
    .await;
    match pulled {
        Ok(img) => {
            let (inner, title) = status_ok(&format!("updated {img}"));
            broadcast_update_out(&unit, &inner, &title, &config);
            end_pull(&config, &unit);
            broadcast_unit_update(&unit, &config).await;
            schedule_unit_refresh_burst(unit, config);
        }
        Err(e) => fail_pull(&unit, &e, &config).await,
    }
}
