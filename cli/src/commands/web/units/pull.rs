//! Container image pull + restart background job.
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, BufReader as AsyncBufReader};
use tokio::process::Command as AsyncCommand;

use super::super::types::AppConfig;
use super::super::util::{status_err, status_ok, status_pulling, sudo_cmd};
use super::control::{
    broadcast_unit_update, broadcast_update_out, end_pull, schedule_unit_refresh_burst,
};

/// Push an error status for a container pull, clear in-flight, and refresh controls.
fn fail_pull(unit: &str, msg: &str, config: &AppConfig) {
    let (inner, title) = status_err(msg);
    broadcast_update_out(unit, &inner, &title, config);
    end_pull(config, unit);
    broadcast_unit_update(unit, config);
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
