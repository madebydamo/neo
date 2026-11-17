//! The config repo on the installer: staged from what was given (URL,
//! checkout, bare repo or plain copy) and the remote of the installed copy.

use anyhow::{bail, Result};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::utils::command::{execute_command, stdout_checked};
use crate::utils::settings::FILE as SETTINGS_FILE;

pub(super) fn git(repo: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C")
        .arg(repo)
        .args(["-c", "safe.directory=*", "-c", "user.name=neo restore"])
        .args(["-c", "user.email=neo@localhost"]);
    c
}

/// A git repository with its own history here: a checkout (`.git` is a folder,
/// or a file in a linked worktree) or a bare repo. Not a folder that merely
/// lies inside some other checkout.
fn is_git_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
        || ["HEAD", "objects", "refs"]
            .iter()
            .all(|n| dir.join(n).exists())
}

/// Put the config repo into `dest` (must not exist) as a git repo, whatever it
/// was given as: a URL, a checkout (its HEAD), a bare repo, or a plain copy of
/// the files without `.git` (one new commit), such as a saved backup of them.
pub(crate) fn stage_repo(repo: &str, dest: &Path) -> Result<()> {
    let dir = Path::new(repo);
    if dir.is_dir() && !is_git_repo(dir) {
        if !dir.join(SETTINGS_FILE).exists() {
            bail!(
                "{repo} is neither a git repository nor a copy of a config repo (no settings.toml)"
            );
        }
        println!("{repo} has no git history: restoring its files as a new commit");
        fs::create_dir_all(dest)?;
        execute_command(Command::new("cp").arg("-a").arg(dir.join(".")).arg(dest))?;
        return commit_uncommitted(dest);
    }
    if dir.join(".git").exists() {
        let dirty = stdout_checked(git(dir).args(["status", "--porcelain"]), "git status")?;
        if !dirty.trim().is_empty() {
            println!("! {repo} has uncommitted changes: only its last commit is restored (commit them first to include them)");
        }
    }
    println!("Cloning {repo} …");
    execute_command(
        Command::new("git")
            .args([
                "-c",
                "safe.directory=*",
                "clone",
                "-q",
                "--no-hardlinks",
                repo,
            ])
            .arg(dest),
    )?;
    if !dest.join(SETTINGS_FILE).exists() {
        bail!("{repo} has no settings.toml in its last commit: is this the config repo?");
    }
    Ok(())
}

/// Remote of the installed repo when none is asked for: a URL stays, a local
/// repo passes on its own `origin` unless that is just another local path.
pub(crate) fn default_origin(repo: &str) -> String {
    let dir = Path::new(repo);
    if !dir.is_dir() {
        return repo.to_string();
    }
    if !is_git_repo(dir) {
        return String::new();
    }
    let url = remote_url(dir);
    if url.starts_with('/') || url.starts_with('.') || url.starts_with("file:") {
        return String::new();
    }
    url
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::fsutil::TempDir;

    /// A small checkout at `dir` holding settings.toml, with one commit.
    fn checkout(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("settings.toml"), "[core]\nhostname = \"a\"\n").unwrap();
        commit_uncommitted(dir).unwrap();
    }

    fn committed(repo: &Path, file: &str) -> bool {
        git(repo)
            .args(["ls-files", "--error-unmatch", file])
            .output()
            .unwrap()
            .status
            .success()
    }

    #[test]
    fn stages_a_checkout_at_its_last_commit() {
        let t = TempDir::new("neo-stage-test").unwrap();
        let src = t.join("src");
        checkout(&src);
        fs::write(src.join("settings.toml"), "dirty = true\n").unwrap();
        fs::write(src.join("untracked"), "x").unwrap();
        let dest = t.join("dest");
        stage_repo(src.to_str().unwrap(), &dest).unwrap();
        let text = fs::read_to_string(dest.join("settings.toml")).unwrap();
        assert!(text.contains("hostname"), "HEAD is restored: {text}");
        assert!(!dest.join("untracked").exists());
    }

    #[test]
    fn stages_a_bare_repo() {
        let t = TempDir::new("neo-stage-test").unwrap();
        let src = t.join("src");
        checkout(&src);
        let bare = t.join("bare.git");
        execute_command(
            Command::new("git")
                .args(["clone", "-q", "--bare"])
                .arg(&src)
                .arg(&bare),
        )
        .unwrap();
        assert!(is_git_repo(&bare));
        let dest = t.join("dest");
        stage_repo(bare.to_str().unwrap(), &dest).unwrap();
        assert!(committed(&dest, "settings.toml"));
    }

    #[test]
    fn stages_a_plain_copy_with_every_file_committed() {
        let t = TempDir::new("neo-stage-test").unwrap();
        let src = t.join("copy");
        fs::create_dir_all(src.join("modules")).unwrap();
        fs::write(src.join("settings.toml"), "x = 1\n").unwrap();
        fs::write(src.join("modules/a.nix"), "{}\n").unwrap();
        fs::write(src.join(".sops.yaml"), "y\n").unwrap();
        let dest = t.join("dest");
        stage_repo(src.to_str().unwrap(), &dest).unwrap();
        for f in ["settings.toml", "modules/a.nix", ".sops.yaml"] {
            assert!(committed(&dest, f), "{f}");
        }
        assert!(!src.join(".git").exists(), "the source stays untouched");
        // A clone of it (what the install does next) works.
        let again = t.join("again");
        stage_repo(dest.to_str().unwrap(), &again).unwrap();
        assert!(again.join("modules/a.nix").exists());
    }

    #[test]
    fn refuses_what_is_no_config_repo() {
        let t = TempDir::new("neo-stage-test").unwrap();
        let empty = t.join("empty");
        fs::create_dir_all(&empty).unwrap();
        let e = stage_repo(empty.to_str().unwrap(), &t.join("d1")).unwrap_err();
        assert!(e.to_string().contains("no settings.toml"), "{e}");
        // A git repo without settings.toml.
        let other = t.join("other");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("readme"), "x").unwrap();
        commit_uncommitted(&other).unwrap();
        let e = stage_repo(other.to_str().unwrap(), &t.join("d2")).unwrap_err();
        assert!(e.to_string().contains("no settings.toml"), "{e}");
    }

    #[test]
    fn origin_defaults() {
        let t = TempDir::new("neo-origin-test").unwrap();
        let url = "git@github.com:me/config.git";
        assert_eq!(default_origin(url), url);
        let plain = t.join("plain");
        fs::create_dir_all(&plain).unwrap();
        assert_eq!(default_origin(plain.to_str().unwrap()), "");
        let repo = t.join("repo");
        checkout(&repo);
        assert_eq!(default_origin(repo.to_str().unwrap()), "");
        execute_command(git(&repo).args(["remote", "add", "origin", url])).unwrap();
        assert_eq!(default_origin(repo.to_str().unwrap()), url);
        execute_command(git(&repo).args(["remote", "set-url", "origin", "/old/path"])).unwrap();
        assert_eq!(default_origin(repo.to_str().unwrap()), "");
    }
}
