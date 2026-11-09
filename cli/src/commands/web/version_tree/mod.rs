//! Unified version tree for the Versioning tab: the configuration history
//! (git commits of the config repo, a DAG with branches from restores) and the
//! system lineage (NixOS generations, branching after rollbacks) on one shared
//! timeline, with links between each generation and the commit it was built
//! from. Pure logic lives in [`model`] and [`lanes`]; this module gathers the
//! inputs from git and the system profile.
mod changes;
mod lanes;
mod model;
pub mod notes;

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant, UNIX_EPOCH};

pub use model::TreeView;
use model::{build_tree, CommitIn, GenIn, TreeInput};

use super::git::{activation_tips, git_stdout, is_worktree_dirty};
use crate::utils::generation::{generation_links, ACTIVATION_LOG};
use crate::utils::{
    current_generation_number, parse_generation_from_message, running_generation_number,
    system_profile_available, GenerationTimeline,
};

/// Rows per page ("Show older" adds another page).
pub const PAGE: usize = 40;
/// Upper bound for `limit` from the client.
pub const MAX_LIMIT: usize = 2000;
/// HEAD reflog entries used to infer the config behind unrecorded generations.
const REFLOG_LIMIT: usize = 400;
/// Activation records come from the journal (slow); reuse them while the
/// generation links and Neo's activation log are unchanged.
const EVENTS_TTL: Duration = Duration::from_secs(600);

type Events = Vec<(i64, Option<u64>)>;
static EVENTS_CACHE: Mutex<Option<(String, Instant, Events)>> = Mutex::new(None);

fn activation_events(sudo_cmd: &str, links_key: &str) -> Events {
    let log_key = std::fs::metadata(ACTIVATION_LOG)
        .ok()
        .map(|m| {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("{}:{}", m.len(), mtime)
        })
        .unwrap_or_default();
    let key = format!("{links_key}|{log_key}");
    if let Ok(guard) = EVENTS_CACHE.lock() {
        if let Some((k, at, ev)) = guard.as_ref() {
            if *k == key && at.elapsed() < EVENTS_TTL {
                return ev.clone();
            }
        }
    }
    let events = GenerationTimeline::load(sudo_cmd).events().to_vec();
    if let Ok(mut guard) = EVENTS_CACHE.lock() {
        *guard = Some((key, Instant::now(), events.clone()));
    }
    events
}

/// `%H %P%x00%s%x00%ct` lines → commits (branch tips filled in by caller).
fn parse_log(raw: &str) -> Vec<CommitIn> {
    raw.lines()
        .filter_map(|line| {
            let mut parts = line.split('\0');
            let left = parts.next()?;
            let subject = parts.next()?;
            let ts = parts.next()?.trim().parse::<i64>().unwrap_or(0);
            let mut ids = left.split_whitespace();
            let id = ids.next()?.to_string();
            Some(CommitIn {
                id,
                parents: ids.map(str::to_string).collect(),
                subject: subject.to_string(),
                timestamp: ts,
                branches: Vec::new(),
                generation: parse_generation_from_message(subject),
                changes: None,
            })
        })
        .collect()
}

/// `<sha>\0HEAD@{<epoch>}` lines from `git log -g --date=unix`.
fn parse_reflog(raw: &str) -> Vec<(i64, String)> {
    raw.lines()
        .filter_map(|line| {
            let (id, sel) = line.split_once('\0')?;
            let t = sel.split_once("@{")?.1.strip_suffix('}')?.parse().ok()?;
            Some((t, id.trim().to_string()))
        })
        .collect()
}

const LOG_FORMAT: &str = "--pretty=format:%H %P%x00%s%x00%ct";

/// Commit (id, time) whose message records generation `n`.
fn commit_for_generation(dir: &Path, n: u64) -> Option<(String, i64)> {
    let grep = format!("(generation {n})");
    let raw = git_stdout(
        dir,
        &[
            "log",
            "-1",
            "-F",
            "--grep",
            &grep,
            "--pretty=format:%H%x00%ct",
            "HEAD",
            "--branches=activation_*",
        ],
    )
    .ok()?;
    let (id, t) = raw.lines().next()?.split_once('\0')?;
    Some((id.to_string(), t.trim().parse().unwrap_or(0)))
}

/// Config-repo half of the inputs.
struct GitInputs {
    head: String,
    commits: Vec<CommitIn>,
    head_generation: Option<u64>,
    reflog: Vec<(i64, String)>,
}

fn gather_git(dir: &Path, limit: usize) -> GitInputs {
    let head = git_stdout(dir, &["rev-parse", "--verify", "-q", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let count = format!("-{}", limit + 1);
    let mut args = vec!["log", count.as_str(), "--date-order", LOG_FORMAT];
    if !head.is_empty() {
        args.push("HEAD");
    }
    args.push("--branches=activation_*");
    let mut commits = parse_log(&git_stdout(dir, &args).unwrap_or_default());
    let tips = activation_tips(dir);
    let pairs: Vec<(String, Option<String>)> = commits
        .iter()
        .map(|c| (c.id.clone(), c.parents.first().cloned()))
        .collect();
    let mut summaries = changes::summarize_commits(dir, &pairs);
    for c in &mut commits {
        if let Some(b) = tips.get(&c.id) {
            c.branches = b.clone();
        }
        c.changes = summaries.remove(&c.id);
    }
    let head_generation = if head.is_empty() {
        None
    } else {
        git_stdout(dir, &["log", "-1", "--format=%B", "HEAD"])
            .ok()
            .and_then(|m| parse_generation_from_message(&m))
    };
    let reflog = if head.is_empty() {
        Vec::new()
    } else {
        let n = format!("-{REFLOG_LIMIT}");
        parse_reflog(
            &git_stdout(
                dir,
                &["log", "-g", &n, "--date=unix", "--format=%H%x00%gd", "HEAD"],
            )
            .unwrap_or_default(),
        )
    };
    GitInputs {
        head,
        commits,
        head_generation,
        reflog,
    }
}

/// Gather everything and build the view (`limit` rows, newest first).
pub fn load(config_path: &str, limit: usize, sudo_cmd: &str) -> TreeView {
    let dir = Path::new(config_path);
    let limit = limit.clamp(1, MAX_LIMIT);
    let GitInputs {
        head,
        commits,
        head_generation,
        reflog,
    } = gather_git(dir, limit);

    let system_available = system_profile_available();
    let links = if system_available {
        generation_links()
    } else {
        Vec::new()
    };
    let current = current_generation_number();
    let running = running_generation_number();
    let gens: Vec<GenIn> = links
        .iter()
        .map(|l| GenIn {
            number: l.number,
            created: l.created,
            target: l.target.clone(),
            is_current: Some(l.number) == current,
            is_running: Some(l.number) == running,
        })
        .collect();
    let events = if gens.is_empty() {
        Vec::new()
    } else {
        let key: Vec<String> = links
            .iter()
            .map(|l| format!("{}@{}", l.number, l.created))
            .collect();
        activation_events(sudo_cmd, &key.join(","))
    };
    let running_commit_fallback = running
        .filter(|r| !commits.iter().any(|c| c.generation == Some(*r)))
        .and_then(|r| commit_for_generation(dir, r));

    let gen_created: Vec<(u64, i64)> = links.iter().map(|l| (l.number, l.created)).collect();
    let mut view = build_tree(&TreeInput {
        commits,
        head,
        head_generation,
        dirty: !config_path.is_empty() && is_worktree_dirty(std::path::Path::new(config_path)),
        system_available,
        system_message: (!system_available).then(|| {
            "This machine has no NixOS system profile (local or development setup).".to_string()
        }),
        gens,
        events,
        reflog,
        running_commit_fallback,
        limit,
    });
    view.notes = notes::load().view(&gen_created);
    view
}

/// Generation `n` and its link creation time, if it exists.
pub fn generation_created(n: u64) -> Option<i64> {
    generation_links()
        .into_iter()
        .find(|l| l.number == n)
        .map(|l| l.created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn parses_log_and_reflog_lines() {
        let log = "aaa bbb ccc\0Merge\x001700000000\nbbb\0Activation: activation_x (generation 4)\x001690000000";
        let c = parse_log(log);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].parents, vec!["bbb", "ccc"]);
        assert_eq!(c[1].generation, Some(4));
        assert_eq!(c[1].timestamp, 1690000000);
        let r = parse_reflog("abc\0HEAD@{1790624778}\nbad line\ndef\0HEAD@{x}");
        assert_eq!(r, vec![(1790624778, "abc".to_string())]);
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .current_dir(dir)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "git {args:?}");
    }

    /// Restore-then-edit in a real repo: the loader sees two lanes.
    #[test]
    fn loads_branching_history_from_git() {
        let tmp = std::env::temp_dir().join(format!("neo-vtree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let d = tmp.as_path();
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        git(d, &["init", "-q", "-b", "master"]);
        std::fs::write(d.join("settings.toml"), "a = 1\n").unwrap();
        git(d, &["add", "."]);
        git(d, &["switch", "-q", "-c", "activation_1"]);
        git(
            d,
            &[
                "commit",
                "-q",
                "-m",
                "Activation: activation_1 (generation 1)",
            ],
        );
        std::fs::write(d.join("settings.toml"), "a = 2\n").unwrap();
        git(d, &["switch", "-q", "-c", "activation_2"]);
        git(
            d,
            &[
                "commit",
                "-q",
                "-am",
                "Activation: activation_2 (generation 2)",
            ],
        );
        // Restore activation_1, re-activate (empty commit), then edit.
        git(d, &["switch", "-q", "activation_1"]);
        git(d, &["switch", "-q", "-c", "activation_3"]);
        git(
            d,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "Activation: activation_3 (generation 3)",
            ],
        );
        std::fs::write(d.join("settings.toml"), "a = 3\n").unwrap();
        git(d, &["switch", "-q", "-c", "activation_4"]);
        git(
            d,
            &[
                "commit",
                "-q",
                "-am",
                "Activation: activation_4 (generation 4)",
            ],
        );

        let g = gather_git(d, 50);
        let _ = std::fs::remove_dir_all(&tmp);
        assert_eq!(g.head_generation, Some(4));
        assert!(!g.reflog.is_empty());
        let t = build_tree(&TreeInput {
            head: g.head,
            commits: g.commits,
            head_generation: g.head_generation,
            reflog: g.reflog,
            limit: 50,
            ..Default::default()
        });
        assert_eq!(t.rows.len(), 4);
        assert_eq!(t.config_graph.lanes, 2);
        let head = t
            .rows
            .iter()
            .find_map(|r| r.commit.as_ref().filter(|c| c.is_head));
        assert_eq!(head.map(|c| c.generation), Some(Some(4)));
        assert!(t
            .rows
            .iter()
            .all(|r| r.commit.as_ref().is_some_and(|c| c.is_tip)));
        // Change summaries: head changed a (settings section), restore was a no-op.
        let by_gen = |n: u64| {
            t.rows
                .iter()
                .find_map(|r| r.commit.as_ref().filter(|c| c.generation == Some(n)))
                .and_then(|c| c.changes.clone())
                .unwrap()
        };
        assert!(by_gen(1).initial);
        assert_eq!(by_gen(4).sections, vec!["a"]);
        assert!(by_gen(3).unchanged);
    }
}
