use std::path::Path;
use std::process::{Command, Output};

use crate::commands::get_current_branch;

use super::structs::{AppConfig, BranchInfo};
use super::util::config_dir;

fn git_output(dir: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {}", args.join(" "), e))
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

pub fn get_activation_graph(config_path: &str) -> String {
    match git_output(
        Path::new(config_path),
        &[
            "log",
            "--graph",
            "--no-color",
            "--oneline",
            "--decorate",
            "-25",
            "--branches=activation_*",
        ],
    ) {
        Ok(o) => {
            let mut t = String::from_utf8_lossy(&o.stdout).into_owned();
            if !o.stderr.is_empty() {
                t.push_str(&String::from_utf8_lossy(&o.stderr));
            }
            t
        }
        Err(e) => format!("graph error: {}", e),
    }
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
