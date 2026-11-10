// Persistent `nix repl` processes behind neo-web's extract queries.
//
// A process loads the config flake once as `f0`. Each evaluation the pool asks
// for is bound as `f_<generation>`: either `f0` itself, or, when the config's
// settings.nix takes `neoSettingsDir`, `f0` re-evaluated over a copy of the
// settings files (no new process, no flake copy; parsed files stay cached).
// See docs/design/eval-latency.md.
mod process;

pub(crate) use process::write_extract_files;
use process::{looks_like_terminal_nix_error, tail_chars, trim_for_error};
// Stderr is collected and drives fail-fast on evaluation errors so the UI never
// waits the full marker timeout while a hard Nix error already completed.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, Command as TokioCommand};
use tokio::time::timeout;

use super::super::icons::IconSet;
use super::registry::NIX_EXTRACTORS;
use crate::commands::web::util::escape_nix_string;

/// How long to keep reading stderr after the first `error:` line so multi-line
/// Nix traces finish before we abort the wait.
const STDERR_ERROR_SETTLE: Duration = Duration::from_millis(200);

/// Text in `modules/settings.nix` that marks support for in-process rebinds.
const REBIND_MARKER: &str = "neoSettingsDir";

/// How every repl of this neo-web loads the config flake.
#[derive(Debug)]
pub struct ReplSpec {
    pub nix_cmd: String,
    /// Absolute config repo directory (the flake).
    pub config_dir: String,
    /// Store path of the neo flake this system was built from (`NEO_FLAKE_PATH`).
    /// When set, the repl loads the host config with `--override-input neo` so
    /// option extract matches the running generation (not a stale flake.lock).
    pub neo_flake_path: Option<String>,
    /// Extractor files (written once).
    pub extract_dir: PathBuf,
    /// Indirect GC roots for the flake sources each repl evaluated.
    pub roots_dir: PathBuf,
}

impl ReplSpec {
    /// Resolve a filesystem path for a `path:` flake URL (absolute, no NUL).
    fn path_flake_url(path: &str) -> Result<String> {
        let p = PathBuf::from(path);
        let abs = if p.is_absolute() {
            p
        } else {
            std::env::current_dir()
                .with_context(|| format!("cwd for path flake {path}"))?
                .join(p)
        };
        let s = abs.to_str().context("path flake is not valid UTF-8")?;
        if s.contains('\0') || s.chars().any(|c| c.is_control()) {
            bail!("path flake has control characters: {s}");
        }
        Ok(format!("path:{s}"))
    }

    /// Installable arguments: the host config with neo overridden to the running
    /// system's neo source. builtins.getFlake ignores CLI overrides, so the
    /// installable must be the config path itself. Empty without NEO_FLAKE_PATH.
    fn installable_args(&self) -> Result<Vec<String>> {
        let Some(neo_src) = self.neo_flake_path.as_deref() else {
            return Ok(vec![]);
        };
        Ok(vec![
            "--override-input".into(),
            "neo".into(),
            Self::path_flake_url(neo_src)?,
            Self::path_flake_url(&self.config_dir)?,
        ])
    }

    /// Whether the config's settings.nix reads `neoSettingsDir`.
    pub fn supports_rebind(&self) -> bool {
        fs::read_to_string(Path::new(&self.config_dir).join("modules/settings.nix"))
            .is_ok_and(|s| s.contains(REBIND_MARKER))
    }

    fn nix_store_cmd(&self) -> PathBuf {
        let nix = Path::new(&self.nix_cmd);
        if nix.is_absolute() {
            nix.with_file_name("nix-store")
        } else {
            PathBuf::from("nix-store")
        }
    }
}

pub struct NixEvaluator {
    /// Store files published while extracting service icons.
    pub icons: Arc<IconSet>,
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    spec: Arc<ReplSpec>,
    /// Number of this process (logs, GC root names).
    pub serial: u64,
    /// Structure hash of the files `f0` was loaded from (snapshot.rs).
    pub structure: u64,
    /// `f0` can take other settings without a reload.
    pub supports_rebind: bool,
    /// Generations bound in this process (`f_<id>`), oldest first.
    generations: Vec<u64>,
    /// The generation queries read.
    flake_var: String,
    /// A query left the process unusable (timeout, dead process, missing store path).
    pub broken: bool,
    /// Append-only buffer of all stderr from this process.
    stderr_buf: Arc<Mutex<String>>,
}

impl std::fmt::Debug for NixEvaluator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NixEvaluator")
            .field("serial", &self.serial)
            .field("generations", &self.generations)
            .finish()
    }
}

impl Drop for NixEvaluator {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.try_wait();
        remove_roots(&self.spec.roots_dir, self.serial);
    }
}

impl NixEvaluator {
    /// Start a repl and load the config flake as `f0`. `why` is logged.
    pub async fn spawn(
        spec: Arc<ReplSpec>,
        icons: Arc<IconSet>,
        serial: u64,
        structure: u64,
        why: &str,
    ) -> Result<Self> {
        let started = Instant::now();
        println!("web: starting nix repl #{serial} ({why})");
        let supports_rebind = spec.supports_rebind();
        let (child, stdin, stdout, stderr) = Self::spawn_repl_process(&spec)?;
        let mut this = NixEvaluator {
            icons,
            child,
            stdin,
            stdout,
            spec,
            serial,
            structure,
            supports_rebind,
            generations: vec![],
            flake_var: "f0".into(),
            broken: false,
            stderr_buf: Arc::new(Mutex::new(String::new())),
        };
        let flake_secs = this.initialize_repl(stderr).await?;
        println!(
            "web: nix repl #{serial} ready in {:.1}s (flake {flake_secs:.1}s, rebind {})",
            started.elapsed().as_secs_f64(),
            if supports_rebind {
                "yes"
            } else {
                "no: full reload per change"
            },
        );
        Ok(this)
    }

    fn spawn_repl_process(
        spec: &ReplSpec,
    ) -> Result<(
        Child,
        tokio::process::ChildStdin,
        BufReader<tokio::process::ChildStdout>,
        BufReader<ChildStderr>,
    )> {
        let mut cmd = TokioCommand::new(&spec.nix_cmd);
        cmd.args([
            "repl",
            "--extra-experimental-features",
            "nix-command flakes",
            "--impure",
            "--quiet",
        ]);
        cmd.args(spec.installable_args()?);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.env("TERM", "dumb");
        cmd.kill_on_drop(true);

        let mut c = cmd
            .spawn()
            .with_context(|| format!("failed to spawn nix repl process using {}", spec.nix_cmd))?;
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
                    if looks_like_terminal_nix_error(&collected)
                        && !collected.contains(marker)
                        && error_seen_at.is_none()
                    {
                        error_seen_at = Some(tokio::time::Instant::now());
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

    async fn send_line(&mut self, line: &str) -> Result<()> {
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Queue a top-level binding (may be long/silent), then a marker print.
    /// The marker only evaluates after the binding finishes — so we truly wait for getFlake.
    async fn bind_then_marker(
        &mut self,
        bind_cmd: &str,
        marker: &str,
        overall: Duration,
    ) -> Result<()> {
        self.send_line(bind_cmd).await?;
        self.print_marker(marker, overall).await
    }

    async fn print_marker(&mut self, marker: &str, overall: Duration) -> Result<()> {
        self.send_line(&format!(r#":p "{marker}""#)).await?;
        self.wait_for_marker(marker, overall).await.map(|_| ())
    }

    /// Handshake, bind `configDir` + `f0`, import the extractors. Returns the
    /// seconds the flake load took.
    async fn initialize_repl(&mut self, err_reader: BufReader<ChildStderr>) -> Result<f64> {
        let buf = self.stderr_buf.clone();
        let serial = self.serial;
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
                            println!("[nix #{serial}] {}", trimmed);
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

        // Flake load via CLI installable (override-input path) can take a long time
        // on a cold cache; allow up to 10 minutes before the first handshake marker.
        let handshake = if self.spec.neo_flake_path.is_some() {
            Duration::from_secs(600)
        } else {
            Duration::from_secs(30)
        };
        for _ in 0..40 {
            let mut line = String::new();
            if timeout(Duration::from_millis(30), self.stdout.read_line(&mut line))
                .await
                .is_err()
            {
                break;
            }
        }

        let started = Instant::now();
        self.print_marker("REPL_READY_FOR_BINDINGS", handshake)
            .await
            .context("repl handshake")?;

        let config_dir = escape_nix_string(&self.spec.config_dir);
        // Top-level REPL bindings (persist for the life of this process).
        self.bind_then_marker(
            &format!(r#"configDir = "{config_dir}""#),
            "CONFIG_DIR_BOUND",
            Duration::from_secs(30),
        )
        .await
        .context("bind configDir")?;

        // Prefer the installable loaded with --override-input neo (matches the
        // running system). builtins.getFlake ignores CLI overrides and would
        // show locked options from flake.lock instead.
        let bind = if self.spec.neo_flake_path.is_some() {
            r#"f0 = { inherit nixosConfigurations; }"#
        } else {
            r#"f0 = builtins.getFlake (builtins.toString (/. + configDir))"#
        };
        self.send_line(bind).await?;
        // Bindings are lazy: force the flake (fetch, lock, outputs) before the
        // marker so a broken flake fails here and the log shows its load time.
        self.send_line(r#":p builtins.seq f0.nixosConfigurations "F_BOUND""#)
            .await?;
        self.wait_for_marker("F_BOUND", Duration::from_secs(600))
            .await
            .context("bind flake f0")?;
        let flake_secs = started.elapsed().as_secs_f64();

        let import_dir = self.spec.extract_dir.display().to_string();
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
        Ok(flake_secs)
    }

    /// Bind generation `id`: `f0` re-evaluated over the settings files in
    /// `settings_dir` when this process supports it, else `f0` as loaded.
    pub async fn bind(&mut self, id: u64, settings_dir: Option<&Path>) -> Result<()> {
        let expr = match settings_dir {
            Some(dir) if self.supports_rebind => {
                let dir = escape_nix_string(&dir.display().to_string());
                format!(
                    r#"f_{id} = f0 // {{ nixosConfigurations = builtins.mapAttrs (_: c: c.extendModules {{ modules = [ {{ {REBIND_MARKER} = "{dir}"; }} ]; }}) f0.nixosConfigurations; }}"#
                )
            }
            _ => format!("f_{id} = f0"),
        };
        self.bind_then_marker(&expr, &format!("GEN_{id}_BOUND"), Duration::from_secs(60))
            .await
            .with_context(|| format!("bind generation {id}"))?;
        self.generations.push(id);
        Ok(())
    }

    pub fn has_gen(&self, id: u64) -> bool {
        self.generations.contains(&id)
    }

    /// Number of generations bound so far. Bindings are never freed by the repl
    /// (rebinding a name keeps the old value reachable), so the pool recycles
    /// a process after a few.
    pub fn bound_count(&self) -> usize {
        self.generations.len()
    }

    /// Resident memory of the repl process in bytes (0 when unknown).
    pub fn rss_bytes(&self) -> u64 {
        let Some(pid) = self.child.id() else {
            return 0;
        };
        fs::read_to_string(format!("/proc/{pid}/status"))
            .ok()
            .and_then(|s| {
                s.lines()
                    .find_map(|l| l.strip_prefix("VmRSS:"))
                    .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            })
            .map(|kb| kb * 1024)
            .unwrap_or(0)
    }

    /// Point the next queries at generation `id` (must be bound).
    pub fn use_gen(&mut self, id: u64) {
        self.flake_var = format!("f_{id}");
    }

    /// The bound flake the extractors read (`neoFlake = <this>`).
    pub(crate) fn flake(&self) -> &str {
        &self.flake_var
    }

    pub(crate) async fn query_json<T: serde::de::DeserializeOwned>(
        &mut self,
        inner: &str,
    ) -> Result<T> {
        if self.broken {
            bail!("nix repl #{} is restarting", self.serial);
        }
        let mut result = self.query_json_once(inner).await;

        // Transient network/fetch failures: one automatic retry before surfacing.
        if let Err(ref e) = result {
            let kind = super::errors::classify(&format!("{e:#}")).kind;
            if kind == super::errors::NixErrorKind::NetworkFetchFailed {
                println!(
                    "web: network/fetch failure during eval; retrying once after short backoff…"
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
                result = self.query_json_once(inner).await;
                if result.is_ok() {
                    println!("web: eval succeeded on network retry");
                }
            }
        }

        if let Err(e) = &result {
            if repl_needs_restart(e) {
                println!("web: nix repl #{} unusable after: {e:#}", self.serial);
                self.broken = true;
            } else {
                // An error in the evaluated config (or in reading its result): the
                // repl is back at its prompt and its bindings are still valid.
                // Sync once to drain the error first.
                println!("web: nix evaluation failed ({e:#}); keeping repl");
                if let Err(se) = self
                    .print_marker("NEO_RESYNC", Duration::from_secs(30))
                    .await
                {
                    println!("web: repl resync after eval error failed ({se:#})");
                    self.broken = true;
                }
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
        self.send_line(&full).await?;
        // 10 minutes: first-time flake evals (full module system + many services)
        // or after nix gc / stale locks can legitimately take a long time.
        let out = self
            .wait_for_marker(&marker, Duration::from_secs(600))
            .await?;
        let json_line = out
            .lines()
            .rev()
            .find(|l| l.contains(&marker))
            .context("missing marker line")?
            .trim_end();
        let v: serde_json::Value = serde_json::from_str(json_line).context("parse marked json")?;
        let res = v.get("result").cloned().context("missing result")?;
        let t: T = serde_json::from_value(res)?;
        Ok(t)
    }

    /// Keep the flake sources this repl evaluated alive across `nix-collect-garbage`:
    /// without roots, GC deletes the config flake's store copy and the next lazily
    /// forced file fails with "path … does not exist". Runs in the background;
    /// the pool starts it after an evaluation so the two do not compete.
    pub fn add_gc_roots(&self) {
        let spec = self.spec.clone();
        let serial = self.serial;
        tokio::spawn(async move {
            match root_flake_sources(&spec, serial).await {
                Ok(n) => println!("web: nix repl #{serial}: {n} flake sources rooted against GC"),
                Err(e) => println!("web: nix repl #{serial}: GC roots not added: {e:#}"),
            }
        });
    }
}

/// `nix flake archive --dry-run --json` lists the flake and all its inputs
/// (already fetched by the repl); each store path gets an indirect root.
async fn root_flake_sources(spec: &ReplSpec, serial: u64) -> Result<usize> {
    let mut cmd = TokioCommand::new(&spec.nix_cmd);
    cmd.args([
        "flake",
        "archive",
        "--dry-run",
        "--json",
        // Only list: never write the config repo's flake.lock.
        "--no-write-lock-file",
        "--extra-experimental-features",
        "nix-command flakes",
    ]);
    let installable = spec.installable_args()?;
    if installable.is_empty() {
        cmd.arg(&spec.config_dir);
    } else {
        cmd.args(installable);
    }
    cmd.kill_on_drop(true);
    let out = cmd.output().await.context("run nix flake archive")?;
    if !out.status.success() {
        bail!(
            "nix flake archive failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let tree: serde_json::Value =
        serde_json::from_slice(&out.stdout).context("parse nix flake archive output")?;
    let mut paths = Vec::new();
    collect_paths(&tree, &mut paths);
    paths.sort();
    paths.dedup();
    // Inputs the evaluation never needed are not fetched (nothing to keep).
    paths.retain(|p| Path::new(p).exists());
    fs::create_dir_all(&spec.roots_dir)?;
    let store = spec.nix_store_cmd();
    let mut rooted = 0;
    for (i, p) in paths.iter().enumerate() {
        let link = spec.roots_dir.join(format!("repl-{serial}-{i}"));
        let st = TokioCommand::new(&store)
            .arg("--add-root")
            .arg(&link)
            .args(["--realise", p])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output()
            .await
            .context("run nix-store --add-root")?;
        // One unregistered path (e.g. seen through a shared store) must not
        // leave the others unrooted.
        if st.status.success() {
            rooted += 1;
        } else {
            println!(
                "web: nix repl #{serial}: no GC root for {p}: {}",
                String::from_utf8_lossy(&st.stderr)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
            );
        }
    }
    Ok(rooted)
}

fn collect_paths(v: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(p) = v.get("path").and_then(|p| p.as_str()) {
        if p.starts_with("/nix/store/") {
            out.push(p.to_string());
        }
    }
    if let Some(inputs) = v.get("inputs").and_then(|i| i.as_object()) {
        for i in inputs.values() {
            collect_paths(i, out);
        }
    }
}

/// Remove the GC roots of repl `serial` (or every repl's with `None`).
pub(crate) fn remove_roots_matching(dir: &Path, serial: Option<u64>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let prefix = match serial {
        Some(s) => format!("repl-{s}-"),
        None => "repl-".to_string(),
    };
    for e in rd.flatten() {
        if e.file_name().to_string_lossy().starts_with(&prefix) {
            let _ = fs::remove_file(e.path());
        }
    }
}

fn remove_roots(dir: &Path, serial: u64) {
    remove_roots_matching(dir, Some(serial));
}

/// Whether a failed query left the repl unusable: a timeout, a closed or
/// broken pipe, or a store path GC removed under it. Errors in the evaluated
/// config, our own result parsing, lock contention or a failed download leave
/// the repl at its prompt with its flake still bound.
fn repl_needs_restart(err: &anyhow::Error) -> bool {
    use super::errors::{classify, NixErrorKind};
    let msg = format!("{err:#}");
    // Only `wait_for_marker`'s stderr fail-fast produces this prefix.
    if msg.starts_with("nix error:") {
        return classify(&msg).kind == NixErrorKind::MissingStorePath;
    }
    msg.starts_with("timeout waiting for marker")
        || msg.contains("stdout closed")
        || msg.contains("Broken pipe")
        || msg.contains("os error")
}

#[cfg(test)]
mod tests {
    use super::process::looks_like_terminal_nix_error;
    use super::{collect_paths, repl_needs_restart};

    #[test]
    fn config_eval_errors_keep_the_repl() {
        for msg in [
            "nix error: error: attribute 'foo' missing",
            "nix error: error: infinite recursion encountered",
            "nix error: error: undefined variable 'x'",
            "nix error: error: assertion '(cfg.port > 0)' failed",
            "nix error: error: unable to download 'https://example.org/x': HTTP error 503",
            "nix error: error: SQLite database is busy",
            "parse marked json: expected value at line 1 column 1",
            "invalid type: string \"x\", expected a sequence",
        ] {
            assert!(!repl_needs_restart(&anyhow::anyhow!("{msg}")), "{msg}");
        }
    }

    #[test]
    fn broken_repl_or_store_restarts() {
        for msg in [
            "timeout waiting for marker __NEO_EVAL_1; output so far: ; stderr: ",
            "nix repl stdout closed while waiting for __NEO_EVAL_1; stderr: ",
            "nix error: error: path '/nix/store/abc-source/flake.nix' does not exist",
            "Broken pipe (os error 32)",
        ] {
            assert!(repl_needs_restart(&anyhow::anyhow!("{msg}")), "{msg}");
        }
    }

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

    #[test]
    fn archive_paths_are_collected_recursively() {
        let v = serde_json::json!({
            "path": "/nix/store/a-source",
            "inputs": {
                "neo": {"path": "/nix/store/b-source", "inputs": {"x": {"path": "/nix/store/c-source", "inputs": {}}}},
                "odd": {"path": "relative", "inputs": {}}
            }
        });
        let mut out = vec![];
        collect_paths(&v, &mut out);
        out.sort();
        assert_eq!(
            out,
            [
                "/nix/store/a-source",
                "/nix/store/b-source",
                "/nix/store/c-source"
            ]
        );
    }
}
