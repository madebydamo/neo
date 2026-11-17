//! The config repo taken out of a backup: commit what it had not committed
//! and read its remote.

use anyhow::Result;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::utils::command::{execute_command, stdout_checked};

/// `git -C <repo>` that works on repos owned by someone else and commits
/// as "neo restore".
pub(crate) fn git(repo: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C")
        .arg(repo)
        .args(["-c", "safe.directory=*", "-c", "user.name=neo restore"])
        .args(["-c", "user.email=neo@localhost"]);
    c
}

/// Commit what a config repo had not committed yet (settings saved in the web
/// UI before the last backup), so the clone carries it. Without `.git` (a copy
/// of the files only) this starts the history.
pub(crate) fn commit_uncommitted(repo: &Path) -> Result<()> {
    let fresh = !repo.join(".git").exists();
    if fresh {
        execute_command(git(repo).args(["init", "-q"]))?;
    }
    let status = stdout_checked(git(repo).args(["status", "--porcelain"]), "git status")?;
    if status.trim().is_empty() {
        return Ok(());
    }
    if !fresh {
        println!("  committing changes the backup's repo had not committed yet");
    }
    execute_command(git(repo).args(["add", "-A"]))?;
    execute_command(git(repo).args([
        "commit",
        "-q",
        "-m",
        if fresh {
            "restore: copy of the config repo without git history"
        } else {
            "restore: uncommitted changes from the backup"
        },
    ]))
}

/// `origin` of a repo, "" without one.
pub(crate) fn remote_url(repo: &Path) -> String {
    git(repo)
        .args(["remote", "get-url", "origin"])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}
