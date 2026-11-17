//! Indirect GC roots for the flake sources a repl evaluated, so
//! `nix-collect-garbage` never deletes what it reads lazily.
use std::fs;
use std::path::Path;
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use tokio::process::Command as TokioCommand;

use super::{NixEvaluator, ReplSpec};

impl NixEvaluator {
    /// Keep the flake sources this repl evaluated alive across `nix-collect-garbage`:
    /// without roots, GC deletes the config flake's store copy and the next lazily
    /// forced file fails with "path … does not exist". Runs in the background;
    /// the pool starts it after an evaluation so the two do not compete.
    pub fn add_gc_roots(&self) {
        let spec = self.spec.clone();
        let serial = self.serial;
        tokio::spawn(async move {
            match root_flake_sources(&spec, serial).await {
                Ok(n) => eprintln!("web: nix repl #{serial}: {n} flake sources rooted against GC"),
                Err(e) => eprintln!("web: nix repl #{serial}: GC roots not added: {e:#}"),
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
            eprintln!(
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

pub(super) fn remove_roots(dir: &Path, serial: u64) {
    remove_roots_matching(dir, Some(serial));
}

#[cfg(test)]
mod tests {
    use super::collect_paths;

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
