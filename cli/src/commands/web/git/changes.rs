//! Collect a structured [`ChangeSet`] from git for the changes preview:
//! pending worktree changes (tracked + untracked) against `HEAD`, or rev → rev.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use super::super::diff::flake_lock::{input_updates, InputUpdate};
use super::super::diff::model::{ChangeSet, DiffLine, FileChange, FileStatus, Hunk, LineKind};
use super::super::diff::parse::{
    parse_name_status_z, parse_numstat_z, parse_patch, parse_status_z, NumstatEntry,
};
use super::super::diff::{diff_settings_toml, SettingsDiff};
use super::plumbing::{git_output, git_stdout};

/// Context lines requested from git; the renderer folds long unchanged runs.
const CONTEXT_ARG: &str = "-U20";
/// Untracked files larger than this are listed without content.
const MAX_UNTRACKED_BYTES: u64 = 512 * 1024;
/// At most this many untracked files get their content loaded.
const MAX_UNTRACKED_FILES: usize = 200;
const SETTINGS_FILE: &str = "settings.toml";
const LOCK_FILE: &str = "flake.lock";

/// Which changes to collect.
#[derive(Debug, Clone, Copy)]
pub enum DiffRange<'a> {
    /// Working tree (staged + unstaged + untracked) against `HEAD`.
    Worktree,
    /// Committed changes between two revisions.
    Revs { from: &'a str, to: &'a str },
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    git_stdout(dir, args).map_err(|e| anyhow!(e))
}

/// `HEAD`, or the empty tree for a repository without commits.
fn worktree_base(dir: &Path) -> Result<String> {
    let has_head = git_output(dir, &["rev-parse", "--verify", "-q", "HEAD^{commit}"])
        .map(|o| o.status.success())
        .unwrap_or(false);
    if has_head {
        return Ok("HEAD".to_string());
    }
    let empty = git(dir, &["hash-object", "-t", "tree", "/dev/null"])
        .context("resolve empty tree for repository without commits")?;
    Ok(empty.trim().to_string())
}

fn diff_args<'a>(revs: &'a [String], extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "-c",
        "core.quotePath=false",
        "diff",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        // Explicit prefixes: user configs like diff.mnemonicPrefix / diff.noprefix
        // would otherwise change the `a/` `b/` markers the patch parser strips.
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "-M",
    ];
    args.extend_from_slice(extra);
    args.extend(revs.iter().map(String::as_str));
    args
}

fn count_lines(hunks: &[Hunk]) -> (u32, u32) {
    let mut a = 0u32;
    let mut d = 0u32;
    for l in hunks.iter().flat_map(|h| &h.lines) {
        match l.kind {
            LineKind::Add => a += 1,
            LineKind::Del => d += 1,
            LineKind::Context => {}
        }
    }
    (a, d)
}

/// Top level of the work tree containing `dir` (`dir` itself if git cannot tell).
fn toplevel(dir: &Path) -> PathBuf {
    git(dir, &["rev-parse", "--show-toplevel"])
        .map(|s| PathBuf::from(s.trim()))
        .unwrap_or_else(|_| dir.to_path_buf())
}

/// Untracked file entry without content.
fn untracked_stub(path: &str) -> FileChange {
    FileChange {
        path: path.to_string(),
        old_path: None,
        status: FileStatus::Untracked,
        additions: 0,
        deletions: 0,
        binary: false,
        similarity: None,
        mode_change: None,
        hunks: Vec::new(),
        omitted: None,
    }
}

/// Content of an untracked file as an all-added change.
fn untracked_change(root: &Path, rel: &str) -> FileChange {
    let mut fc = untracked_stub(rel);
    let full: PathBuf = root.join(rel);
    let meta = match std::fs::symlink_metadata(&full) {
        Ok(m) => m,
        Err(e) => {
            fc.omitted = Some(format!("Could not read file: {e}"));
            return fc;
        }
    };
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(&full)
            .map(|t| t.to_string_lossy().into_owned())
            .unwrap_or_default();
        fc.omitted = Some(format!("Symbolic link → {target}"));
        return fc;
    }
    if meta.is_dir() {
        fc.omitted = Some("Directory (nested repository) — contents not shown.".to_string());
        return fc;
    }
    if meta.len() > MAX_UNTRACKED_BYTES {
        fc.omitted = Some(format!(
            "Large new file ({} KiB) — contents not shown.",
            meta.len() / 1024
        ));
        return fc;
    }
    let bytes = match std::fs::read(&full) {
        Ok(b) => b,
        Err(e) => {
            fc.omitted = Some(format!("Could not read file: {e}"));
            return fc;
        }
    };
    if bytes.iter().take(8000).any(|b| *b == 0) {
        fc.binary = true;
        return fc;
    }
    let text = String::from_utf8_lossy(&bytes);
    if text.is_empty() {
        return fc;
    }
    let body = text.strip_suffix('\n').unwrap_or(&text);
    let mut lines: Vec<DiffLine> = body
        .split('\n')
        .enumerate()
        .map(|(i, l)| DiffLine {
            kind: LineKind::Add,
            old_no: None,
            new_no: Some(u32::try_from(i + 1).unwrap_or(u32::MAX)),
            text: l.to_string(),
            no_newline: false,
        })
        .collect();
    if !text.ends_with('\n') {
        if let Some(last) = lines.last_mut() {
            last.no_newline = true;
        }
    }
    let n = u32::try_from(lines.len()).unwrap_or(u32::MAX);
    fc.additions = n;
    fc.hunks = vec![Hunk {
        old_start: 0,
        old_len: 0,
        new_start: 1,
        new_len: n,
        section: String::new(),
        lines,
    }];
    fc
}

/// Collect all changed files with counts and parsed hunks.
pub fn collect_changes(dir: &Path, range: DiffRange) -> Result<ChangeSet> {
    let revs: Vec<String> = match range {
        DiffRange::Worktree => vec![worktree_base(dir)?],
        DiffRange::Revs { from, to } => vec![from.to_string(), to.to_string()],
    };

    let numstat =
        git(dir, &diff_args(&revs, &["--numstat", "-z"])).context("git diff --numstat")?;
    let names =
        git(dir, &diff_args(&revs, &["--name-status", "-z"])).context("git diff --name-status")?;
    let patch = git(dir, &diff_args(&revs, &[CONTEXT_ARG])).context("git diff (patch)")?;

    let nums: HashMap<String, NumstatEntry> = parse_numstat_z(&numstat)
        .into_iter()
        .map(|n| (n.path.clone(), n))
        .collect();
    let mut patches: HashMap<String, _> = parse_patch(&patch)
        .into_iter()
        .filter_map(|p| Some((p.key_path()?.to_string(), p)))
        .collect();

    let mut files = Vec::new();
    for ns in parse_name_status_z(&names) {
        let p = patches.remove(&ns.path);
        let num = nums.get(&ns.path);
        let hunks = p.as_ref().map(|p| p.hunks.clone()).unwrap_or_default();
        let (pa, pd) = count_lines(&hunks);
        let binary_num = num.is_some_and(|n| n.additions.is_none());
        let binary = binary_num || p.as_ref().is_some_and(|p| p.binary);
        let mode_change = p.as_ref().and_then(|p| match (&p.old_mode, &p.new_mode) {
            (Some(a), Some(b)) if a != b => Some((a.clone(), b.clone())),
            _ => None,
        });
        files.push(FileChange {
            additions: num.and_then(|n| n.additions).unwrap_or(pa),
            deletions: num.and_then(|n| n.deletions).unwrap_or(pd),
            path: ns.path,
            old_path: ns.old_path,
            status: ns.status,
            binary,
            similarity: ns.score.or(p.as_ref().and_then(|p| p.similarity)),
            mode_change,
            hunks,
            omitted: None,
        });
    }
    // Sections git printed in the patch but not in --name-status (defensive).
    for (path, p) in patches {
        let (a, d) = count_lines(&p.hunks);
        files.push(FileChange {
            path,
            old_path: p
                .old_path
                .clone()
                .filter(|o| Some(o) != p.new_path.as_ref()),
            status: p.status.unwrap_or(FileStatus::Modified),
            additions: a,
            deletions: d,
            binary: p.binary,
            similarity: p.similarity,
            mode_change: None,
            hunks: p.hunks,
            omitted: None,
        });
    }

    if matches!(range, DiffRange::Worktree) {
        let status = git(
            dir,
            &[
                "-c",
                "core.quotePath=false",
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
            ],
        )
        .context("git status")?;
        let root = toplevel(dir);
        let untracked: Vec<String> = parse_status_z(&status)
            .into_iter()
            .filter(|e| e.is_untracked())
            .map(|e| e.path)
            .collect();
        for (i, path) in untracked.into_iter().enumerate() {
            if i >= MAX_UNTRACKED_FILES {
                files.push(FileChange {
                    omitted: Some("Too many untracked files — contents not loaded.".into()),
                    ..untracked_stub(&path)
                });
                continue;
            }
            files.push(untracked_change(&root, path.trim_end_matches('/')));
        }
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(ChangeSet { files })
}

/// `settings.toml` at a rev (`""` when absent there).
fn settings_at(dir: &Path, rev: &str) -> String {
    file_at(dir, rev, SETTINGS_FILE)
}

/// A file at a rev (`""` when absent there).
fn file_at(dir: &Path, rev: &str, file: &str) -> String {
    git(dir, &["show", &format!("{rev}:{file}")]).unwrap_or_default()
}

fn touches(set: &ChangeSet, file: &str) -> bool {
    set.files
        .iter()
        .any(|f| f.path == file || f.old_path.as_deref() == Some(file))
}

/// Updated flake inputs when `flake.lock` is part of `set`.
pub fn lock_updates(dir: &Path, set: &ChangeSet, range: DiffRange) -> Vec<InputUpdate> {
    if !touches(set, LOCK_FILE) {
        return Vec::new();
    }
    let (old, new) = match range {
        DiffRange::Worktree => (
            worktree_base(dir)
                .map(|b| file_at(dir, &b, LOCK_FILE))
                .unwrap_or_default(),
            std::fs::read_to_string(toplevel(dir).join(LOCK_FILE)).unwrap_or_default(),
        ),
        DiffRange::Revs { from, to } => {
            (file_at(dir, from, LOCK_FILE), file_at(dir, to, LOCK_FILE))
        }
    };
    input_updates(&old, &new)
}

/// Semantic settings summary when `settings.toml` is part of `set`.
pub fn settings_semantic(
    dir: &Path,
    set: &ChangeSet,
    range: DiffRange,
) -> Option<Result<SettingsDiff, String>> {
    if !touches(set, SETTINGS_FILE) {
        return None;
    }
    let texts: Result<(String, String)> = match range {
        DiffRange::Worktree => {
            let old = worktree_base(dir)
                .map(|b| settings_at(dir, &b))
                .unwrap_or_default();
            let path = toplevel(dir).join(SETTINGS_FILE);
            match std::fs::read_to_string(&path) {
                Ok(new) => Ok((old, new)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((old, String::new())),
                Err(e) => Err(anyhow!(e)).with_context(|| format!("read {}", path.display())),
            }
        }
        DiffRange::Revs { from, to } => Ok((settings_at(dir, from), settings_at(dir, to))),
    };
    Some(
        texts
            .and_then(|(o, n)| diff_settings_toml(&o, &n))
            .map_err(|e| format!("{e:#}")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn run(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "git {:?} failed", args);
    }

    /// End-to-end against a throwaway repo (skipped when git is unavailable).
    #[test]
    fn collects_modified_deleted_renamed_and_untracked() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!(
            "neo-changes-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&tmp).expect("tmpdir");
        run(&tmp, &["init", "-q"]);
        run(&tmp, &["config", "user.email", "t@example.com"]);
        run(&tmp, &["config", "user.name", "t"]);
        std::fs::write(tmp.join("settings.toml"), "[services.a]\nenabled = false\n")
            .expect("write");
        std::fs::write(tmp.join("gone.txt"), "bye\n").expect("write");
        std::fs::write(
            tmp.join("old name.nix"),
            "{ a = 1; b = 2; c = 3; d = 4; }\n{ e = 5; }\n",
        )
        .expect("write");
        run(&tmp, &["add", "-A"]);
        run(
            &tmp,
            &[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--no-verify",
                "-m",
                "init",
            ],
        );

        std::fs::write(tmp.join("settings.toml"), "[services.a]\nenabled = true\n").expect("write");
        std::fs::remove_file(tmp.join("gone.txt")).expect("rm");
        run(&tmp, &["mv", "old name.nix", "new name.nix"]);
        std::fs::create_dir_all(tmp.join("plugins")).expect("mkdir");
        std::fs::write(tmp.join("plugins/new.txt"), "x\ny").expect("write");

        let set = collect_changes(&tmp, DiffRange::Worktree).expect("collect");
        let by = |p: &str| set.files.iter().find(|f| f.path == p).expect(p);
        assert_eq!(by("settings.toml").status, FileStatus::Modified);
        assert_eq!(
            (by("settings.toml").additions, by("settings.toml").deletions),
            (1, 1)
        );
        assert_eq!(by("gone.txt").status, FileStatus::Deleted);
        let ren = by("new name.nix");
        assert_eq!(ren.status, FileStatus::Renamed);
        assert_eq!(ren.old_path.as_deref(), Some("old name.nix"));
        let un = by("plugins/new.txt");
        assert_eq!(un.status, FileStatus::Untracked);
        assert_eq!(un.additions, 2);
        assert!(un.hunks[0].lines[1].no_newline);

        let sem = settings_semantic(&tmp, &set, DiffRange::Worktree)
            .expect("touched")
            .expect("parsed");
        assert_eq!(sem.services.len(), 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
