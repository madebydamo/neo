use std::fs;
use std::mem;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, Command as TokioCommand};
use tokio::time::timeout;

use super::registry::NIX_EXTRACTORS;

pub struct NixEvaluator {
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    nix_cmd: String,
    neo_input: String,
    eval_dir: PathBuf,
    busy: Arc<AtomicBool>,
    last_config_mtime: SystemTime,
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

        let (child, stdin, stdout, stderr) = Self::spawn_repl_process(nix_cmd)?;
        let mut this = NixEvaluator {
            child,
            stdin,
            stdout,
            nix_cmd: nix_cmd.to_string(),
            neo_input: neo_input.to_string(),
            eval_dir,
            busy: busy.clone(),
            last_config_mtime: current_config_mtime(neo_input),
        };

        this.initialize_repl(stderr).await?;

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

    async fn initialize_repl(&mut self, err_reader: BufReader<ChildStderr>) -> Result<()> {
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
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        self.busy.store(true, Ordering::Relaxed);
        for _ in 0..40 {
            let mut line = String::new();
            if timeout(Duration::from_millis(30), self.stdout.read_line(&mut line))
                .await
                .is_err()
            {
                break;
            }
        }

        let _ = self.execute(r#":p "REPL_READY_FOR_BINDINGS""#).await;

        let config_dir = &self.neo_input;
        let _ = self
            .execute(&format!(r#"configDir = "{}""#, config_dir))
            .await;
        let _ = self
            .execute(r#"f = builtins.getFlake (builtins.toString (/. + configDir))"#)
            .await;
        let _ = self.execute(r#":p "F_BOUND""#).await;

        let import_dir = self.eval_dir.display().to_string();
        for e in NIX_EXTRACTORS {
            let _ = self
                .execute(&format!(
                    "{} = import {}/{}",
                    e.load_name, import_dir, e.file_name
                ))
                .await;
        }

        let _ = self.execute(r#":p "NEO_REPL_READY""#).await;
        self.busy.store(false, Ordering::Relaxed);

        Ok(())
    }

    async fn execute(&mut self, cmd: &str) -> Result<String> {
        self.busy.store(true, Ordering::Relaxed);
        self.stdin.write_all(cmd.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        let mut collected = String::new();
        let mut line = String::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
        while tokio::time::Instant::now() < deadline {
            line.clear();
            match timeout(Duration::from_millis(150), self.stdout.read_line(&mut line)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(_)) => {
                    collected.push_str(&line);
                    let trimmed = line.trim_end();
                    if trimmed.contains("nix-repl>")
                        || (collected.len() > 2 && line.trim().is_empty())
                    {
                        break;
                    }
                }
                _ => break,
            }
        }
        self.busy.store(false, Ordering::Relaxed);
        Ok(collected)
    }

    pub(crate) async fn query_json<T: serde::de::DeserializeOwned>(
        &mut self,
        inner: &str,
    ) -> Result<T> {
        self.busy.store(true, Ordering::Relaxed);

        if let Err(e) = self.auto_refresh_if_stale().await {
            println!("web: auto mtime-based refresh check failed: {e}");
        }

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
        self.stdin.write_all(full.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        println!("web: starting nix evaluation");
        let mut collected = String::new();
        let mut line = String::new();
        let mut found: Option<String> = None;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(600); // 10 minutes; first-time flake evals (full module system + many services) or after nix gc / stale locks can legitimately take a long time. We surface errors gracefully instead of hanging the UI.
        while tokio::time::Instant::now() < deadline {
            line.clear();
            match timeout(Duration::from_millis(250), self.stdout.read_line(&mut line)).await {
                Ok(Ok(0)) => break,
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
        self.busy.store(false, Ordering::Relaxed);
        let json_line = match found {
            Some(l) => l,
            None => {
                if collected.contains("error:") {
                    println!("web: nix returned error during evaluation: {}", &collected);
                    anyhow::bail!("nix error: {}", &collected);
                }
                println!("web: nix evaluation FAILED to find marker within deadline. Output so far (last 2k): {}", &collected.chars().rev().take(2000).collect::<String>().chars().rev().collect::<String>());
                anyhow::bail!("no marker from repl, output: {}", &collected);
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
            println!("web: detected updated config files on disk -- restarting nix repl process before serving results");
            let _ = self.refresh().await;
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
