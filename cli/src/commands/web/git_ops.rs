use std::process::Command;

use crate::commands::get_current_branch;

use super::structs::{AppConfig, BranchInfo};
use super::util::config_dir;

pub fn list_activation_branches(config_path: &str) -> Vec<BranchInfo> {
    let names: Vec<String> = Command::new("git")
        .current_dir(config_path)
        .args([
            "for-each-ref",
            "--format=%(refname:short)",
            "--sort=-committerdate",
            "refs/heads/activation_*",
        ])
        .output()
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
    let out = Command::new("git")
        .current_dir(config_path)
        .args([
            "log",
            "--graph",
            "--no-color",
            "--oneline",
            "--decorate",
            "-25",
            "--branches=activation_*",
        ])
        .output();
    match out {
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

pub fn worktree_changed_and_summary(cfg: &AppConfig) -> (bool, String) {
    let dir = config_dir(cfg);
    let staged = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false);
    let unstaged = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false);
    let changed = staged || unstaged;
    if !changed {
        return (false, String::new());
    }
    let status = Command::new("git")
        .current_dir(&dir)
        .args(["status", "--porcelain", "-b", "--short"])
        .output();
    let stat = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--stat", "--no-color"])
        .output();
    let mut text = String::new();
    if let Ok(o) = status {
        text.push_str(&String::from_utf8_lossy(&o.stdout));
    }
    text.push_str("\n");
    if let Ok(o) = stat {
        text.push_str(&String::from_utf8_lossy(&o.stdout));
    }
    (true, text)
}

pub fn settings_toml_has_diff(cfg: &AppConfig) -> bool {
    let dir = config_dir(cfg);
    let unstaged = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--quiet", "--", "settings.toml"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false);
    let staged = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--cached", "--quiet", "--", "settings.toml"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false);
    unstaged || staged
}

pub fn get_settings_toml_diff(cfg: &AppConfig) -> String {
    let dir = config_dir(cfg);
    let output = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--no-color", "HEAD", "--", "settings.toml"])
        .output();
    match output {
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
