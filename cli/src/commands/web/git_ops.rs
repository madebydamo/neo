use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Output};

use toml_edit::DocumentMut;

use crate::commands::generation::parse_generation_from_message;
use crate::commands::get_current_branch;

use super::structs::{AppConfig, BranchInfo, GraphCommit, ServicesAtRev, VersioningGraph};
use super::util::config_dir;

const GRAPH_LIMIT: usize = 80;

fn git_output(dir: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {}", args.join(" "), e))
}

fn git_stdout(dir: &Path, args: &[&str]) -> Result<String, String> {
    let o = git_output(dir, args)?;
    if !o.status.success() {
        let err = String::from_utf8_lossy(&o.stderr);
        return Err(if err.trim().is_empty() {
            format!("git {} failed", args.join(" "))
        } else {
            err.trim().to_string()
        });
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// True when staged or unstaged changes exist (`pathspec` limits the check when set).
fn git_dirty(dir: &Path, pathspec: Option<&str>) -> bool {
    let is_dirty = |cached: bool| {
        let mut args: Vec<&str> = vec!["diff"];
        if cached {
            args.push("--cached");
        }
        args.push("--quiet");
        if let Some(p) = pathspec {
            args.push("--");
            args.push(p);
        }
        git_output(dir, &args)
            .map(|o| !o.status.success())
            .unwrap_or(false)
    };
    is_dirty(true) || is_dirty(false)
}

fn worktree_summary(dir: &Path) -> String {
    let mut text = String::new();
    if let Ok(o) = git_output(dir, &["status", "--porcelain", "-b", "--short"]) {
        text.push_str(&String::from_utf8_lossy(&o.stdout));
    }
    text.push('\n');
    if let Ok(o) = git_output(dir, &["diff", "--stat", "--no-color"]) {
        text.push_str(&String::from_utf8_lossy(&o.stdout));
    }
    text
}

/// Combined dirty flags for action-bar / changes UI (one pass of git checks).
pub struct DirtyState {
    pub settings_dirty: bool,
    pub worktree_dirty: bool,
    pub summary: String,
}

/// Run settings + worktree dirty checks efficiently (summary only when worktree dirty).
pub fn dirty_state(cfg: &AppConfig) -> DirtyState {
    let dir = config_dir(cfg);
    let settings_dirty = git_dirty(&dir, Some("settings.toml"));
    let worktree_dirty = git_dirty(&dir, None);
    let summary = if worktree_dirty {
        worktree_summary(&dir)
    } else {
        String::new()
    };
    DirtyState {
        settings_dirty,
        worktree_dirty,
        summary,
    }
}

pub fn is_worktree_dirty(config_path: &str) -> bool {
    git_dirty(Path::new(config_path), None)
}

pub fn list_activation_branches(config_path: &str) -> Vec<BranchInfo> {
    let names: Vec<String> = git_output(
        Path::new(config_path),
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "--sort=-committerdate",
            "refs/heads/activation_*",
        ],
    )
    .map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect()
    })
    .unwrap_or_default();
    let cur = get_current_branch(config_path).unwrap_or_default();
    names
        .into_iter()
        .map(|name| BranchInfo {
            name: name.clone(),
            is_current: name == cur,
        })
        .collect()
}

pub fn get_settings_toml_diff(cfg: &AppConfig) -> String {
    let dir = config_dir(cfg);
    match git_output(&dir, &["diff", "--no-color", "HEAD", "--", "settings.toml"]) {
        Ok(o) => {
            let mut t = String::from_utf8_lossy(&o.stdout).into_owned();
            let e = String::from_utf8_lossy(&o.stderr);
            if !e.is_empty() {
                t.push_str(&e);
            }
            t
        }
        Err(e) => format!("git diff error: {}", e),
    }
}

/// Resolve a safe-looking rev to a full commit id via shell git.
pub fn resolve_rev(config_path: &str, rev: &str) -> Result<String, String> {
    let out = git_stdout(Path::new(config_path), &["rev-parse", "--verify", &format!("{}^{{commit}}", rev)])?;
    let id = out.trim().to_string();
    if id.len() != 40 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("unexpected rev-parse result for {}", rev));
    }
    Ok(id)
}

/// Structured activation history for the D3 graph (shell git only).
pub fn activation_graph(config_path: &str) -> VersioningGraph {
    let dir = Path::new(config_path);
    let head = git_stdout(dir, &["rev-parse", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let current_branch = get_current_branch(config_path).unwrap_or_default();

    // branch tip → name (may be multiple tips at same commit)
    let mut branch_tips: HashMap<String, Vec<String>> = HashMap::new();
    if let Ok(raw) = git_stdout(
        dir,
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname:short)",
            "refs/heads/activation_*",
        ],
    ) {
        for line in raw.lines() {
            let mut parts = line.splitn(2, ' ');
            if let (Some(oid), Some(name)) = (parts.next(), parts.next()) {
                branch_tips
                    .entry(oid.to_string())
                    .or_default()
                    .push(name.to_string());
            }
        }
    }

    // NUL-separated fields so subjects may contain `|`. Generation is in the subject:
    // `Activation: activation_… (generation N)`.
    // fullsha SP parents %x00 subject %x00 unix_ts  (one commit per line)
    let log = git_stdout(
        dir,
        &[
            "log",
            &format!("-{}", GRAPH_LIMIT),
            "--branches=activation_*",
            "--pretty=format:%H %P%x00%s%x00%ct",
        ],
    )
    .unwrap_or_default();

    let mut commits = Vec::new();
    for line in log.lines() {
        let parts: Vec<&str> = line.split('\0').collect();
        if parts.len() < 3 {
            continue;
        }
        let left = parts[0];
        let subject = parts[1];
        let ts_s = parts[2];
        let mut ids = left.split_whitespace();
        let Some(id) = ids.next() else {
            continue;
        };
        let parents: Vec<String> = ids.map(|s| s.to_string()).collect();
        let timestamp = ts_s.trim().parse::<i64>().unwrap_or(0);
        let branches = branch_tips.get(id).cloned().unwrap_or_default();
        let generation = parse_generation_from_message(subject);
        let short_id = if id.len() >= 7 {
            id[..7].to_string()
        } else {
            id.to_string()
        };
        commits.push(GraphCommit {
            id: id.to_string(),
            short_id,
            parents,
            subject: subject.to_string(),
            timestamp,
            branches,
            is_head: id == head,
            generation,
        });
    }

    VersioningGraph {
        commits,
        head,
        current_branch,
    }
}

/// Read `settings.toml` blob at `rev` (shell `git show`).
pub fn settings_at_rev(config_path: &str, rev: &str) -> Result<String, String> {
    let _ = resolve_rev(config_path, rev)?;
    git_stdout(
        Path::new(config_path),
        &["show", &format!("{}:settings.toml", rev)],
    )
}

/// Parse enabled/disabled services from `settings.toml` at `rev`.
pub fn enabled_services_at_rev(config_path: &str, rev: &str) -> Result<ServicesAtRev, String> {
    let raw = settings_at_rev(config_path, rev)?;
    let doc: DocumentMut = raw
        .parse()
        .map_err(|e| format!("parse settings.toml at {}: {e}", rev))?;
    let mut enabled = Vec::new();
    let mut disabled = Vec::new();
    if let Some(services) = doc.get("services").and_then(|i| i.as_table()) {
        for (name, item) in services.iter() {
            let is_enabled = item
                .as_table()
                .and_then(|t| t.get("enabled"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if is_enabled {
                enabled.push(name.to_string());
            } else {
                disabled.push(name.to_string());
            }
        }
    }
    enabled.sort();
    disabled.sort();
    Ok(ServicesAtRev {
        rev: rev.to_string(),
        enabled,
        disabled,
    })
}

/// Unified diff of `settings.toml` between two revs (`git diff a b -- settings.toml`).
pub fn diff_settings(config_path: &str, a: &str, b: &str) -> Result<String, String> {
    let _ = resolve_rev(config_path, a)?;
    let _ = resolve_rev(config_path, b)?;
    let o = git_output(
        Path::new(config_path),
        &["diff", "--no-color", a, b, "--", "settings.toml"],
    )?;
    // diff exits 1 when differences exist — still success for us
    let code = o.status.code().unwrap_or(1);
    if code != 0 && code != 1 {
        let err = String::from_utf8_lossy(&o.stderr);
        return Err(if err.trim().is_empty() {
            format!("git diff failed (exit {})", code)
        } else {
            err.trim().to_string()
        });
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// Prefer an activation branch name that points at `rev` (full or short sha).
pub fn activation_branch_for_rev(config_path: &str, rev: &str) -> Result<Option<String>, String> {
    let full = resolve_rev(config_path, rev)?;
    let raw = git_stdout(
        Path::new(config_path),
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname:short)",
            "refs/heads/activation_*",
        ],
    )?;
    for line in raw.lines() {
        let mut parts = line.splitn(2, ' ');
        if let (Some(oid), Some(name)) = (parts.next(), parts.next()) {
            if oid == full {
                return Ok(Some(name.to_string()));
            }
        }
    }
    Ok(None)
}
