//! Subprocess helpers. Errors are plain strings: they go straight into status chips.
//! Streaming or capped children (pulls, logs, store repair) spawn their own.
use std::process::Output;

use tokio::process::Command;

use crate::utils::command::sudo_bin;

/// Run blocking work (git, sops, a sync systemd call) off Rocket's async workers,
/// which would otherwise stall unrelated requests and sockets.
pub async fn blocking<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("background task failed: {e}"))
}

/// `sudo -n`: neo-web has no terminal, so a missing sudo rule fails at once.
pub fn sudo() -> Command {
    let mut c = Command::new(sudo_bin());
    c.arg("-n");
    c
}

/// `out` when the command succeeded, else `{what} failed: {stderr}`.
pub fn check(out: Output, what: &str) -> Result<Output, String> {
    if out.status.success() {
        return Ok(out);
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let err = err.trim();
    Err(if err.is_empty() {
        format!("{what} failed ({})", out.status)
    } else {
        format!("{what} failed: {err}")
    })
}

/// Run `cmd`; failing to start it or a non-zero exit is an error. Returns stdout.
pub async fn run_ok(cmd: &mut Command, what: &str) -> Result<String, String> {
    let out = cmd.output().await.map_err(|e| format!("{what}: {e}"))?;
    let out = check(out, what)?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    #[tokio::test]
    async fn run_ok_returns_stdout() {
        assert_eq!(
            run_ok(&mut sh("echo hi"), "greet").await.as_deref(),
            Ok("hi\n")
        );
    }

    #[tokio::test]
    async fn run_ok_reports_stderr() {
        assert_eq!(
            run_ok(&mut sh("echo nope >&2; exit 3"), "greet")
                .await
                .unwrap_err(),
            "greet failed: nope"
        );
    }

    #[tokio::test]
    async fn run_ok_without_stderr_names_the_status() {
        assert!(run_ok(&mut sh("exit 3"), "greet")
            .await
            .unwrap_err()
            .starts_with("greet failed (exit"));
    }

    #[tokio::test]
    async fn run_ok_reports_a_failed_spawn() {
        let err = run_ok(&mut Command::new("/nonexistent/neo-test"), "greet")
            .await
            .unwrap_err();
        assert!(err.starts_with("greet: "), "{err}");
    }
}
