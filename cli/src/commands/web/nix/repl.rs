// Persistent `nix repl` used by neo-web for fast extract queries.
// Stderr is collected and drives fail-fast on evaluation errors so the UI never
// waits the full marker timeout while a hard Nix error already completed.
use std::fs;
use std::mem;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, Command as TokioCommand};
use tokio::time::timeout;

use super::registry::NIX_EXTRACTORS;

/// How long to keep reading stderr after the first `error:` line so multi-line
/// Nix traces finish before we abort the wait.
const STDERR_ERROR_SETTLE: Duration = Duration::from_millis(200);

pub struct NixEvaluator {
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    nix_cmd: String,
    neo_input: String,
    eval_dir: PathBuf,
    busy: Arc<AtomicBool>,
    last_config_mtime: SystemTime,
    /// Append-only buffer of all stderr from the current repl process.
    stderr_buf: Arc<Mutex<String>>,
}

impl std::fmt::Debug for NixEvaluator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NixEvaluator")
            .field("neo_input", &self.neo_input)
            .field("eval_dir", &self.eval_dir)
            .finish()
    }
}

impl Drop for NixEvaluator {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.try_wait();
        let _ = fs::remove_dir_all(&self.eval_dir);
    }
}

impl NixEvaluator {
    pub async fn new(nix_cmd: &str, neo_input: &str, busy: Arc<AtomicBool>) -> Result<Self> {
        let pid = std::process::id();
        let eval_dir = std::env::temp_dir().join(format!("neo-nix-repl-{}", pid));
        write_extract_files(&eval_dir)?;

        let stderr_buf = Arc::new(Mutex::new(String::new()));
        let (child, stdin, stdout, stderr) = Self::spawn_repl_process(nix_cmd)?;
        let mut this = NixEvaluator {
            child,
            stdin,
            stdout,
            nix_cmd: nix_cmd.to_string(),
            neo_input: neo_input.to_string(),
            eval_dir,
            busy: busy.clone(),
            // Set after initialize so getFlake / first force cannot look "stale".
            last_config_mtime: SystemTime::UNIX_EPOCH,
            stderr_buf,
        };

        this.initialize_repl(stderr).await?;
        this.last_config_mtime = current_config_mtime(neo_input);

        Ok(this)
    }

    fn spawn_repl_process(
        nix_cmd: &str,
    ) -> Result<(
        Child,
        tokio::process::ChildStdin,
        BufReader<tokio::process::ChildStdout>,
        BufReader<ChildStderr>,
    )> {
        let mut cmd = TokioCommand::new(nix_cmd);
        cmd.args([
            "repl",
            "--extra-experimental-features",
            "nix-command flakes",
            "--impure",
            "--quiet",
        ]);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.env("TERM", "dumb");

        let mut c = cmd
            .spawn()
            .with_context(|| format!("failed to spawn nix repl process using {}", nix_cmd))?;
        let stdin = c.stdin.take().context("nix repl child has no stdin")?;
        let stdout = BufReader::new(c.stdout.take().context("nix repl child has no stdout")?);
        let stderr = BufReader::new(c.stderr.take().context("nix repl child has no stderr")?);
        Ok((c, stdin, stdout, stderr))
    }

    fn stderr_len(&self) -> usize {
        self.stderr_buf.lock().map(|b| b.len()).unwrap_or(0)
    }

    fn stderr_since(&self, from: usize) -> String {
        self.stderr_buf
            .lock()
            .map(|b| {
                if from >= b.len() {
                    String::new()
                } else {
                    b[from..].to_string()
                }
            })
            .unwrap_or_default()
    }

    fn clear_stderr(&self) {
        if let Ok(mut b) = self.stderr_buf.lock() {
            b.clear();
        }
    }

    /// Wait until stdout contains `marker` (or overall deadline).
    /// Aborts early when Nix writes a terminal `error:` to stderr, or when the process dies.
    async fn wait_for_marker(&mut self, marker: &str, overall: Duration) -> Result<String> {
        let mut collected = String::new();
        let mut line = String::new();
        let deadline = tokio::time::Instant::now() + overall;
        let stderr_start = self.stderr_len();
        let mut error_seen_at: Option<tokio::time::Instant> = None;

        while tokio::time::Instant::now() < deadline {
            // Fail-fast: once stderr shows a terminal error, settle briefly then abort.
            let stderr_slice = self.stderr_since(stderr_start);
            if looks_like_terminal_nix_error(&stderr_slice) {
                match error_seen_at {
                    None => {
                        error_seen_at = Some(tokio::time::Instant::now());
                    }
                    Some(t) if t.elapsed() >= STDERR_ERROR_SETTLE => {
                        let detail = trim_for_error(&stderr_slice);
                        println!("web: nix stderr error during wait for {marker}: {}", detail);
                        bail!("nix error: {}", detail);
                    }
                    Some(_) => {}
                }
            }

            line.clear();
            match timeout(Duration::from_millis(250), self.stdout.read_line(&mut line)).await {
                Ok(Ok(0)) => {
                    // Process exited (or stdout closed) without the marker.
                    let stderr_slice = self.stderr_since(stderr_start);
                    if looks_like_terminal_nix_error(&stderr_slice) {
                        let detail = trim_for_error(&stderr_slice);
                        bail!("nix error: {}", detail);
                    }
                    bail!(
                        "nix repl stdout closed while waiting for {marker}; stderr: {}",
                        trim_for_error(&stderr_slice)
                    );
                }
                Ok(Ok(_)) => {
                    collected.push_str(&line);
                    if line.trim_end().contains(marker) {
                        return Ok(collected);
                    }
                    // Rare: error text on stdout instead of stderr.
                    if looks_like_terminal_nix_error(&collected) && !collected.contains(marker) {
                        if error_seen_at.is_none() {
                            error_seen_at = Some(tokio::time::Instant::now());
                        }
                    }
                }
                // Short read timeout: keep waiting (getFlake / module eval often silent on stdout).
                _ => {}
            }
        }

        let stderr_slice = self.stderr_since(stderr_start);
        if looks_like_terminal_nix_error(&stderr_slice) {
            let detail = trim_for_error(&stderr_slice);
            bail!("nix error: {}", detail);
        }
        if looks_like_terminal_nix_error(&collected) {
            bail!("nix error: {}", trim_for_error(&collected));
        }
        bail!(
            "timeout waiting for marker {marker}; output so far: {}; stderr: {}",
            tail_chars(&collected, 2000),
            trim_for_error(&stderr_slice)
        )
    }

    /// Queue a top-level binding (may be long/silent), then a marker print.
    /// The marker only evaluates after the binding finishes — so we truly wait for getFlake.
    /// Caller owns `busy` (used during initialize_repl / refresh).
    async fn bind_then_marker(
        &mut self,
        bind_cmd: &str,
        marker: &str,
        overall: Duration,
    ) -> Result<()> {
        self.stdin.write_all(bind_cmd.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin
            .write_all(format!(r#":p "{marker}""#).as_bytes())
            .await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        self.wait_for_marker(marker, overall).await.map(|_| ())
    }

    async fn print_marker(&mut self, marker: &str, overall: Duration) -> Result<()> {
        self.stdin
            .write_all(format!(r#":p "{marker}""#).as_bytes())
            .await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        self.wait_for_marker(marker, overall).await.map(|_| ())
    }

    async fn initialize_repl(&mut self, err_reader: BufReader<ChildStderr>) -> Result<()> {
        self.clear_stderr();
        let buf = self.stderr_buf.clone();
        tokio::spawn(async move {
            let mut line = String::new();
            let mut err_reader = err_reader;
            loop {
                line.clear();
                match err_reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim_end();
                        if !trimmed.is_empty() {
                            println!("[nix] {}", trimmed);
                            if let Ok(mut b) = buf.lock() {
                                b.push_str(trimmed);
                                b.push('\n');
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        self.busy.store(true, Ordering::Relaxed);
        let init_result = self.initialize_repl_inner().await;
        if init_result.is_err() {
            self.busy.store(false, Ordering::Relaxed);
        }
        // On success, initialize_repl_inner leaves busy=false via the ready marker path.
        init_result
    }

    async fn initialize_repl_inner(&mut self) -> Result<()> {
        for _ in 0..40 {
            let mut line = String::new();
            if timeout(Duration::from_millis(30), self.stdout.read_line(&mut line))
                .await
                .is_err()
            {
                break;
            }
        }

        self.print_marker("REPL_READY_FOR_BINDINGS", Duration::from_secs(30))
            .await
            .context("repl handshake")?;

        let config_dir = self.neo_input.clone();
        // Top-level REPL bindings (persist for the life of this process).
        self.bind_then_marker(
            &format!(r#"configDir = "{}""#, config_dir),
            "CONFIG_DIR_BOUND",
            Duration::from_secs(30),
        )
        .await
        .context("bind configDir")?;

        // getFlake is often silent on stdout for a long time. Marker is queued *after*
        // the assignment so we only proceed once f is actually bound in the repl.
        println!("web: binding flake f via builtins.getFlake (once per repl process)…");
        self.bind_then_marker(
            r#"f = builtins.getFlake (builtins.toString (/. + configDir))"#,
            "F_BOUND",
            Duration::from_secs(600),
        )
        .await
        .context("bind flake f")?;
        println!(
            "web: flake f bound (memoized in this nix repl process until config mtime refresh)"
        );

        let import_dir = self.eval_dir.display().to_string();
        for e in NIX_EXTRACTORS {
            let marker = format!("LOADED_{}", e.load_name);
            self.bind_then_marker(
                &format!("{} = import {}/{}", e.load_name, import_dir, e.file_name),
                &marker,
                Duration::from_secs(60),
            )
            .await
            .with_context(|| format!("import {}", e.file_name))?;
        }

        self.print_marker("NEO_REPL_READY", Duration::from_secs(30))
            .await
            .context("repl ready")?;
        self.busy.store(false, Ordering::Relaxed);

        Ok(())
    }

    pub(crate) async fn query_json<T: serde::de::DeserializeOwned>(
        &mut self,
        inner: &str,
    ) -> Result<T> {
        self.busy.store(true, Ordering::Relaxed);

        if let Err(e) = self.auto_refresh_if_stale().await {
            println!("web: auto mtime-based refresh check failed: {e}");
        }
        // refresh() clears busy; re-assert for the actual query.
        self.busy.store(true, Ordering::Relaxed);

        let mut result = self.query_json_once(inner).await;

        // Transient network/fetch failures: one automatic retry before surfacing.
        if let Err(ref e) = result {
            let kind = super::errors::NixError::classify(&format!("{e:#}")).kind;
            if kind == super::errors::NixErrorKind::NetworkFetchFailed {
                println!(
                    "web: network/fetch failure during eval; retrying once after short backoff…"
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
                // Keep busy true across the retry.
                result = self.query_json_once(inner).await;
                if result.is_ok() {
                    println!("web: eval succeeded on network retry");
                }
            }
        }

        match &result {
            Ok(_) => {
                self.busy.store(false, Ordering::Relaxed);
            }
            Err(e) => {
                println!(
                    "web: nix evaluation failed ({e:#}); restarting repl so the next request is not stuck"
                );
                if let Err(re) = self.refresh().await {
                    println!("web: repl refresh after eval failure also failed: {re:#}");
                    self.busy.store(false, Ordering::Relaxed);
                }
                // refresh() success path clears busy in initialize_repl.
            }
        }

        result
    }

    async fn query_json_once<T: serde::de::DeserializeOwned>(&mut self, inner: &str) -> Result<T> {
        let marker = format!(
            "__NEO_EVAL_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let full = format!(
            r#":p (builtins.toJSON ({{ __marker = "{}"; result = ({}); }}))"#,
            marker, inner
        );
        let stderr_start = self.stderr_len();
        self.stdin.write_all(full.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        println!(
            "web: starting nix evaluation (uses bound flake f; full getFlake only after config mtime change)"
        );

        let mut collected = String::new();
        let mut line = String::new();
        let mut found: Option<String> = None;
        let mut error_seen_at: Option<tokio::time::Instant> = None;
        // 10 minutes: first-time flake evals (full module system + many services)
        // or after nix gc / stale locks can legitimately take a long time.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(600);

        while tokio::time::Instant::now() < deadline {
            let stderr_slice = self.stderr_since(stderr_start);
            if looks_like_terminal_nix_error(&stderr_slice) {
                match error_seen_at {
                    None => {
                        error_seen_at = Some(tokio::time::Instant::now());
                    }
                    Some(t) if t.elapsed() >= STDERR_ERROR_SETTLE => {
                        let detail = trim_for_error(&stderr_slice);
                        println!("web: nix returned error during evaluation: {}", detail);
                        bail!("nix error: {}", detail);
                    }
                    Some(_) => {}
                }
            }

            line.clear();
            match timeout(Duration::from_millis(250), self.stdout.read_line(&mut line)).await {
                Ok(Ok(0)) => {
                    let stderr_slice = self.stderr_since(stderr_start);
                    if looks_like_terminal_nix_error(&stderr_slice) {
                        let detail = trim_for_error(&stderr_slice);
                        println!("web: nix returned error during evaluation: {}", detail);
                        bail!("nix error: {}", detail);
                    }
                    bail!(
                        "nix repl stdout closed during evaluation; stderr: {}",
                        trim_for_error(&stderr_slice)
                    );
                }
                Ok(Ok(_)) => {
                    collected.push_str(&line);
                    let trimmed = line.trim_end();
                    if trimmed.contains(&marker) {
                        found = Some(trimmed.to_string());
                        println!("web: nix evaluation completed (marker received)");
                        break;
                    }
                }
                _ => {}
            }
        }

        let json_line = match found {
            Some(l) => l,
            None => {
                let stderr_slice = self.stderr_since(stderr_start);
                if looks_like_terminal_nix_error(&stderr_slice) {
                    let detail = trim_for_error(&stderr_slice);
                    println!("web: nix returned error during evaluation: {}", detail);
                    bail!("nix error: {}", detail);
                }
                if looks_like_terminal_nix_error(&collected) {
                    println!("web: nix returned error on stdout: {}", &collected);
                    bail!("nix error: {}", trim_for_error(&collected));
                }
                println!(
                    "web: nix evaluation FAILED to find marker within deadline. Output so far (last 2k): {}; stderr: {}",
                    tail_chars(&collected, 2000),
                    trim_for_error(&stderr_slice)
                );
                bail!(
                    "no marker from repl, output: {}; stderr: {}",
                    collected,
                    trim_for_error(&stderr_slice)
                );
            }
        };
        let v: serde_json::Value = serde_json::from_str(&json_line).context("parse marked json")?;
        let res = v.get("result").cloned().context("missing result")?;
        let t: T = serde_json::from_value(res)?;
        Ok(t)
    }

    async fn auto_refresh_if_stale(&mut self) -> Result<()> {
        let current = current_config_mtime(&self.neo_input);
        let stale = current > self.last_config_mtime;
        if stale {
            println!(
                "web: config-folder mtime advanced — restarting nix repl and rebinding f (was {:?}, now {:?})",
                self.last_config_mtime, current
            );
            // On failure leave last_config_mtime unchanged so the next request retries.
            self.refresh().await?;
        }
        Ok(())
    }

    pub async fn refresh(&mut self) -> Result<()> {
        self.busy.store(true, Ordering::Relaxed);
        let _ = self.child.start_kill();
        let (new_child, new_stdin, new_stdout, new_stderr) =
            Self::spawn_repl_process(&self.nix_cmd)?;

        let mut old_child = mem::replace(&mut self.child, new_child);
        let old_stdin = mem::replace(&mut self.stdin, new_stdin);
        let old_stdout = mem::replace(&mut self.stdout, new_stdout);
        tokio::spawn(async move {
            let _ = old_child.wait().await;
        });
        drop(old_stdin);
        drop(old_stdout);
        if let Err(e) = self.initialize_repl(new_stderr).await {
            self.busy.store(false, Ordering::Relaxed);
            return Err(e);
        }

        self.last_config_mtime = current_config_mtime(&self.neo_input);
        Ok(())
    }
}

/// True when `text` looks like a finished Nix evaluation error (not a progress line).
pub(crate) fn looks_like_terminal_nix_error(text: &str) -> bool {
    for line in text.lines() {
        let t = line.trim_start();
        // Primary form Nix uses for evaluation failures.
        if t.starts_with("error:") {
            return true;
        }
        // Nested / secondary form in multi-line traces.
        if t.starts_with("error: ") || t.contains("error: path ") {
            return true;
        }
    }
    false
}

fn trim_for_error(s: &str) -> String {
    let t = s.trim();
    if t.len() <= 4000 {
        t.to_string()
    } else {
        // Prefer the end of the trace (leaf error).
        tail_chars(t, 4000)
    }
}

fn tail_chars(s: &str, n: usize) -> String {
    s.chars()
        .rev()
        .take(n)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn write_extract_files(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    for e in NIX_EXTRACTORS {
        fs::write(dir.join(e.file_name), e.content)?;
    }
    Ok(())
}

fn current_config_mtime(config_dir: &str) -> SystemTime {
    let root = Path::new(config_dir);
    let mut max_t = SystemTime::UNIX_EPOCH;
    if let Ok(meta) = fs::metadata(root) {
        if let Ok(t) = meta.modified() {
            let root_relevant = meta.is_dir()
                || root
                    .extension()
                    .and_then(|e| e.to_str())
                    .map_or(false, |e| e == "nix" || e == "toml" || e == "lock");
            if root_relevant && t > max_t {
                max_t = t;
            }
        }
        if meta.is_dir() {
            if let Ok(rd) = fs::read_dir(root) {
                for entry in rd.flatten() {
                    walk_mtime(&entry.path(), &mut max_t);
                }
            }
        }
    }
    max_t
}

fn walk_mtime(p: &Path, max_t: &mut SystemTime) {
    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.') || name == "result" || name == "target" {
            return;
        }
    }
    let meta = match fs::metadata(p) {
        Ok(m) => m,
        Err(_) => return,
    };
    if let Ok(t) = meta.modified() {
        let relevant = meta.is_dir()
            || p.extension()
                .and_then(|e| e.to_str())
                .map_or(false, |e| e == "nix" || e == "toml" || e == "lock");
        if relevant && t > *max_t {
            *max_t = t;
        }
    }
    if meta.is_dir() {
        if let Ok(rd) = fs::read_dir(p) {
            for entry in rd.flatten() {
                walk_mtime(&entry.path(), max_t);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::looks_like_terminal_nix_error;

    #[test]
    fn detects_missing_store_path_trace() {
        let sample = r#"
error:
       … while calling the 'toJSON' builtin
       error: path '/nix/store/z10yq3qjir82v7jb3nakx5hm3hr0qv9r-source/flake.nix' does not exist
"#;
        assert!(looks_like_terminal_nix_error(sample));
    }

    #[test]
    fn ignores_innocent_text() {
        assert!(!looks_like_terminal_nix_error("evaluating…\nbuilding…\n"));
        assert!(!looks_like_terminal_nix_error(
            "warning: Git tree is dirty\n"
        ));
    }
}
