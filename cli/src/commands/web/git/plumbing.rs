//! Low-level git command helpers for the web UI.
use std::path::Path;
use std::process::{Command, Output};

pub(crate) fn git_output(dir: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {}", args.join(" "), e))
}

pub(crate) fn git_stdout(dir: &Path, args: &[&str]) -> Result<String, String> {
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
pub(crate) fn git_dirty(dir: &Path, pathspec: Option<&str>) -> bool {
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

pub(crate) fn worktree_summary(dir: &Path) -> String {
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
