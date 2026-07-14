//! Run declared option helper bash scripts with a fixed I/O protocol.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;

const HELPER_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_STDOUT: usize = 64 * 1024;
const MAX_STDERR: usize = 16 * 1024;

pub struct HelperExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

/// Invoke `bash script_path` with JSON stdin; capture stdout/stderr with caps.
pub async fn run_helper_script(
    script: &Path,
    stdin_json: &str,
    env_extra: &[(&str, &str)],
) -> Result<HelperExecResult> {
    if !script.is_absolute() {
        bail!("helper script path must be absolute");
    }
    // Defense-in-depth: release builds only run store-resident helpers.
    // Primary trust is that `script` came from server-side schema, never the client.
    // Debug builds still allow absolute non-store paths for local dev.
    let script_s = script.to_string_lossy();
    if !cfg!(debug_assertions) && !script_s.starts_with("/nix/store/") {
        bail!(
            "helper script must be under /nix/store/ in release builds (got {})",
            script_s
        );
    }

    let bash = std::env::var("NEO_HELPER_BASH").unwrap_or_else(|_| "bash".to_string());

    let tmp = tempfile_dir()?;
    let home = tmp.join("home");
    std::fs::create_dir_all(&home).ok();

    let mut cmd = Command::new(&bash);
    cmd.arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(&tmp)
        .env_clear()
        .env("PATH", helper_path())
        .env("HOME", &home)
        .env("TMPDIR", &tmp)
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8");
    for (k, v) in env_extra {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().context("spawn helper bash")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(stdin_json.as_bytes())
            .await
            .context("write helper stdin")?;
        drop(stdin);
    }

    let run = async {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        if let Some(mut out) = child.stdout.take() {
            let mut buf = vec![0u8; 4096];
            loop {
                let n = out.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                if stdout.len() + n > MAX_STDOUT {
                    stdout
                        .extend_from_slice(&buf[..n.min(MAX_STDOUT.saturating_sub(stdout.len()))]);
                    break;
                }
                stdout.extend_from_slice(&buf[..n]);
            }
        }
        if let Some(mut err) = child.stderr.take() {
            let mut buf = vec![0u8; 4096];
            loop {
                let n = err.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                if stderr.len() + n > MAX_STDERR {
                    stderr
                        .extend_from_slice(&buf[..n.min(MAX_STDERR.saturating_sub(stderr.len()))]);
                    break;
                }
                stderr.extend_from_slice(&buf[..n]);
            }
        }
        let status = child.wait().await?;
        Ok::<_, anyhow::Error>((status, stdout, stderr))
    };

    match timeout(HELPER_TIMEOUT, run).await {
        Ok(Ok((status, stdout, stderr))) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Ok(HelperExecResult {
                exit_code: status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&stdout).into_owned(),
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
                timed_out: false,
            })
        }
        Ok(Err(e)) => {
            let _ = child.kill().await;
            let _ = std::fs::remove_dir_all(&tmp);
            Err(e)
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = std::fs::remove_dir_all(&tmp);
            Ok(HelperExecResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: true,
            })
        }
    }
}

fn tempfile_dir() -> Result<std::path::PathBuf> {
    let base = std::env::temp_dir().join("neo-helper");
    std::fs::create_dir_all(&base).ok();
    let dir = base.join(format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).context("create helper temp dir")?;
    Ok(dir)
}

fn helper_path() -> String {
    // Prefer NEO_HELPER_PATH (explicit tool dirs from neo-web), then process PATH,
    // then common NixOS system paths so local `neo web` and activated units both work.
    let mut parts: Vec<String> = Vec::new();
    if let Ok(p) = std::env::var("NEO_HELPER_PATH") {
        if !p.is_empty() {
            parts.push(p);
        }
    }
    if let Ok(p) = std::env::var("PATH") {
        if !p.is_empty() {
            parts.push(p);
        }
    }
    parts.push("/run/current-system/sw/bin".into());
    parts.push("/usr/bin".into());
    parts.push("/bin".into());
    parts.join(":")
}

/// Parse helper stdout into a JSON value for form fill.
pub fn parse_helper_value(stdout: &str) -> Result<serde_json::Value> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        bail!("helper produced empty output");
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(obj) = v.as_object() {
            if let Some(val) = obj.get("value") {
                return Ok(val.clone());
            }
        }
        // bare JSON string/number/bool/array
        return Ok(v);
    }
    // bare string line
    Ok(serde_json::Value::String(trimmed.to_string()))
}
