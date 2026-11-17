//! The unified version tree: configuration commits (git) and system
//! generations (NixOS) on one shared, newest-first timeline. [`build_tree`]
//! merges both sequences into rows (a commit and the generation it created
//! share a row, rows.rs), assigns lanes to both DAGs and derives the sync /
//! drift status, from the system lineage (lineage.rs) and the commit ↔
//! generation links (links.rs).
use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use super::changes::ChangeSummary;
use super::lanes::{assign_lanes, DagNode, Edge};
use super::lineage::{gen_id, system_lineage, SysNode};
use super::links::{link_generations, CommitGenState, LinkHow, Links};
use super::rows::{merge_rows, RowRef};

/// Id of the synthetic "unapplied changes" node above HEAD.
pub const WORKTREE_ID: &str = "WORKTREE";

// ─── Inputs ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct CommitIn {
    pub id: String,
    pub parents: Vec<String>,
    pub subject: String,
    /// Committer time (epoch seconds).
    pub timestamp: i64,
    /// `activation_*` branches pointing here (restorable tips).
    pub branches: Vec<String>,
    /// Generation recorded in the message by `neo activate`.
    pub generation: Option<u64>,
    /// What changed in `settings.toml` against the first parent.
    pub changes: Option<ChangeSummary>,
}

#[derive(Clone, Debug, Default)]
pub struct GenIn {
    pub number: u64,
    /// Link creation time (epoch seconds).
    pub created: i64,
    /// System store path.
    pub target: String,
    /// Boot default (`/nix/var/nix/profiles/system`).
    pub is_current: bool,
    /// Running now (`/run/current-system`).
    pub is_running: bool,
}

#[derive(Clone, Debug, Default)]
pub struct TreeInput {
    /// Newest first in git `--date-order` (children before parents).
    /// Callers pass one more than `limit` when more history exists.
    pub commits: Vec<CommitIn>,
    pub head: String,
    /// Generation recorded on HEAD (looked up separately, HEAD may be outside
    /// the window).
    pub head_generation: Option<u64>,
    pub dirty: bool,
    pub system_available: bool,
    pub system_message: Option<String>,
    pub gens: Vec<GenIn>,
    /// `(epoch, generation)` activation records sorted by time (boots,
    /// switches); None = a system without a surviving generation.
    pub events: Vec<(i64, Option<u64>)>,
    /// HEAD reflog `(epoch, commit)` (any order).
    pub reflog: Vec<(i64, String)>,
    /// Commit a running generation outside the window was built from
    /// (looked up by `(generation N)` grep).
    pub running_commit_fallback: Option<(String, i64)>,
    /// Maximum rows returned.
    pub limit: usize,
}

// ─── Output ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CommitKind {
    Activation,
    Build,
    Other,
}

pub fn commit_kind(subject: &str) -> CommitKind {
    let s = subject.trim_start();
    if s.len() >= 11 && s[..11].eq_ignore_ascii_case("activation:") {
        CommitKind::Activation
    } else if s.len() >= 6 && s[..6].eq_ignore_ascii_case("build:") {
        CommitKind::Build
    } else {
        CommitKind::Other
    }
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitRef {
    pub id: String,
    pub short: String,
    pub how: LinkHow,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitView {
    pub id: String,
    pub short: String,
    pub parents: Vec<String>,
    pub subject: String,
    pub kind: CommitKind,
    pub timestamp: i64,
    pub branches: Vec<String>,
    /// Has an `activation_*` branch: can be restored.
    pub is_tip: bool,
    pub is_head: bool,
    /// The running system was built from this commit.
    pub is_running_config: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gen_state: Option<CommitGenState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<ChangeSummary>,
    pub lane: usize,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SysView {
    Generation {
        id: String,
        number: u64,
        created: i64,
        is_running: bool,
        is_boot: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        built_from: Option<CommitRef>,
        /// Lineage parent generation (the one running when this was built).
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_gen: Option<u64>,
        /// Built after switching away from the newest generation (rollback).
        branched: bool,
        /// Lower-numbered generation with the identical system.
        #[serde(skip_serializing_if = "Option::is_none")]
        identical_to: Option<u64>,
        lane: usize,
    },
    Switch {
        id: String,
        time: i64,
        to: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        from: Option<u64>,
        /// Target older than the previous system.
        rollback: bool,
        /// Latest switch and its target still runs: "you are here".
        is_latest: bool,
        lane: usize,
    },
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RowView {
    pub key: String,
    pub time: i64,
    /// Synthetic row for unapplied changes (lane in `pending_lane`).
    pub pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_lane: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<CommitView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SysView>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GraphView {
    pub lanes: usize,
    pub edges: Vec<Edge>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StatusKind {
    /// Running system was built from the current settings.
    Sync,
    /// Running system was built from another version (rollback / restore).
    Drift,
    /// Current settings were never built into a system.
    Unbuilt,
    /// Running generation cannot be tied to a version.
    Unknown,
    /// No system generations on this machine.
    Unavailable,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatusView {
    pub kind: StatusKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running: Option<u64>,
    /// Boot default when it differs from the running generation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_boot: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running_commit: Option<CommitRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running_commit_time: Option<i64>,
    /// Generation recorded on HEAD, if it still exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running_since: Option<i64>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub generations: usize,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TreeView {
    pub head: String,
    pub head_short: String,
    pub dirty: bool,
    pub rows: Vec<RowView>,
    pub config_graph: GraphView,
    pub system_graph: GraphView,
    pub system: SystemInfo,
    pub status: StatusView,
    pub has_more: bool,
    pub limit: usize,
    /// User notes on versions and generations (filled by the loader).
    pub notes: NotesView,
}

/// Notes shown on the timeline: commit id → note, generation number → note.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotesView {
    pub commits: BTreeMap<String, String>,
    pub generations: BTreeMap<u64, String>,
}

fn short(id: &str) -> String {
    id.chars().take(7).collect()
}

/// When the running system started (latest record switching to it).
fn running_since(events: &[(i64, Option<u64>)], gens: &[GenIn], running: u64) -> Option<i64> {
    // Walk back over consecutive records of the running generation.
    let mut since = None;
    for (t, g) in events.iter().rev() {
        if *g == Some(running) {
            since = Some(*t);
        } else {
            break;
        }
    }
    since.or_else(|| gens.iter().find(|g| g.number == running).map(|g| g.created))
}

/// A commit and the generation it recorded + built share a row, up to `limit` rows.
/// The bool: more rows exist beyond them.
fn window_rows(
    input: &TreeInput,
    limit: usize,
    links: &Links,
    sys_nodes: &[SysNode],
) -> (Vec<RowRef>, bool) {
    let commits = &input.commits;
    let sys_index: HashMap<String, usize> = sys_nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id(), i))
        .collect();
    let commit_index: HashMap<&str, usize> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    let mut pairs: HashMap<usize, usize> = HashMap::new();
    for (n, link) in &links.by_gen {
        if link.how != LinkHow::Recorded {
            continue;
        }
        if let (Some(&ci), Some(&si)) = (
            commit_index.get(link.commit.as_str()),
            sys_index.get(&gen_id(*n)),
        ) {
            pairs.insert(ci, si);
        }
    }
    let commit_times: Vec<i64> = commits.iter().map(|c| c.timestamp).collect();
    let sys_times: Vec<i64> = sys_nodes.iter().map(|n| n.time()).collect();
    let mut rows = merge_rows(&commit_times, &sys_times, &pairs);

    // Window: `limit` rows, and never past the sentinel commit (the one extra
    // commit fetched beyond the limit) — anything after it may interleave
    // with history that was not loaded.
    let mut cut = rows.len().min(limit);
    if commits.len() > limit {
        if let Some(p) = rows.iter().position(|r| r.commit == Some(limit)) {
            cut = cut.min(p);
        }
    }
    let has_more = rows.len() > cut;
    rows.truncate(cut);
    (rows, has_more)
}

/// Config DAG nodes, with the pending worktree on top when `pending`.
fn config_dag(input: &TreeInput, rows: &[RowRef], pending: bool) -> Vec<DagNode> {
    let offset = usize::from(pending);
    let mut nodes: Vec<DagNode> = Vec::new();
    if pending {
        nodes.push(DagNode {
            row: 0,
            id: WORKTREE_ID.to_string(),
            parents: vec![input.head.clone()],
        });
    }
    for (r, row) in rows.iter().enumerate() {
        if let Some(ci) = row.commit {
            let c = &input.commits[ci];
            nodes.push(DagNode {
                row: r + offset,
                id: c.id.clone(),
                parents: c.parents.clone(),
            });
        }
    }
    nodes
}

/// System DAG nodes (parents outside the window stay open).
fn system_dag(rows: &[RowRef], sys_nodes: &[SysNode], offset: usize) -> Vec<DagNode> {
    let mut nodes: Vec<DagNode> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        if let Some(si) = row.sys {
            let n = &sys_nodes[si];
            nodes.push(DagNode {
                row: r + offset,
                id: n.id(),
                parents: n.parent().cloned().into_iter().collect(),
            });
        }
    }
    nodes
}

/// Running / boot generations and how the running system relates to HEAD.
fn status_view(input: &TreeInput, links: &Links) -> StatusView {
    let commits = &input.commits;
    let running = input.gens.iter().find(|g| g.is_running).map(|g| g.number);
    let boot = input.gens.iter().find(|g| g.is_current).map(|g| g.number);
    let commit_time: HashMap<&str, i64> = commits
        .iter()
        .map(|c| (c.id.as_str(), c.timestamp))
        .collect();
    let running_link = running.and_then(|r| links.by_gen.get(&r).cloned());
    let running_commit: Option<(CommitRef, Option<i64>)> = match (&running_link, running) {
        (Some(l), _) => Some((
            CommitRef {
                id: l.commit.clone(),
                short: short(&l.commit),
                how: l.how,
            },
            commit_time.get(l.commit.as_str()).copied(),
        )),
        (None, Some(_)) => input.running_commit_fallback.as_ref().map(|(id, t)| {
            (
                CommitRef {
                    id: id.clone(),
                    short: short(id),
                    how: LinkHow::Recorded,
                },
                Some(*t),
            )
        }),
        _ => None,
    };
    let gen_target: HashMap<u64, &str> = input
        .gens
        .iter()
        .map(|g| (g.number, g.target.as_str()))
        .collect();
    let head_gen = input.head_generation.filter(|n| gen_target.contains_key(n));
    let identical = |a: u64, b: u64| {
        a == b
            || matches!((gen_target.get(&a), gen_target.get(&b)), (Some(x), Some(y)) if !x.is_empty() && x == y)
    };
    let kind = if !input.system_available || input.gens.is_empty() {
        StatusKind::Unavailable
    } else if let Some(r) = running {
        let built_from_head = running_commit
            .as_ref()
            .is_some_and(|(c, _)| !input.head.is_empty() && c.id == input.head);
        if built_from_head || head_gen.is_some_and(|h| identical(h, r)) {
            StatusKind::Sync
        } else if input.head_generation.is_none() {
            StatusKind::Unbuilt
        } else if running_commit.is_some() || head_gen.is_some() {
            StatusKind::Drift
        } else {
            StatusKind::Unknown
        }
    } else {
        StatusKind::Unknown
    };
    StatusView {
        kind,
        running,
        next_boot: boot.filter(|b| Some(*b) != running),
        running_commit: running_commit.as_ref().map(|(c, _)| c.clone()),
        running_commit_time: running_commit.as_ref().and_then(|(_, t)| *t),
        head_generation: head_gen,
        running_since: running.and_then(|r| running_since(&input.events, &input.gens, r)),
    }
}

fn commit_view(
    c: &CommitIn,
    input: &TreeInput,
    links: &Links,
    running_commit: Option<&str>,
    lane: usize,
) -> CommitView {
    let (generation, gen_state) = match links.by_commit.get(&c.id) {
        Some((n, st)) => (Some(*n), Some(*st)),
        None => (c.generation, None),
    };
    CommitView {
        id: c.id.clone(),
        short: short(&c.id),
        parents: c.parents.clone(),
        subject: c.subject.clone(),
        kind: commit_kind(&c.subject),
        timestamp: c.timestamp,
        branches: c.branches.clone(),
        is_tip: !c.branches.is_empty(),
        is_head: c.id == input.head,
        is_running_config: running_commit == Some(c.id.as_str()),
        generation,
        gen_state,
        changes: c.changes.clone(),
        lane,
    }
}

/// What a row of the system column needs from the whole lineage.
struct SysCx<'a> {
    input: &'a TreeInput,
    links: &'a Links,
    sys_nodes: &'a [SysNode],
    latest_switch: Option<usize>,
}

impl SysCx<'_> {
    /// Newest generation number before `created`, for "branched" (rollback) flags.
    fn newest_before(&self, created: i64) -> Option<u64> {
        self.input
            .gens
            .iter()
            .filter(|g| g.created < created)
            .map(|g| g.number)
            .max()
    }

    fn view(&self, si: usize, lane: usize) -> SysView {
        let (input, links, sys_nodes) = (self.input, self.links, self.sys_nodes);
        let n = &sys_nodes[si];
        let id = n.id();
        match n {
            SysNode::Gen {
                number,
                created,
                parent,
            } => {
                let g = input.gens.iter().find(|g| g.number == *number);
                let parent_gen = parent.as_ref().and_then(|p| {
                    if let Some(num) = p.strip_prefix('g') {
                        num.parse().ok()
                    } else {
                        sys_nodes.iter().find_map(|s| match s {
                            SysNode::Switch { to, .. } if &s.id() == p => Some(*to),
                            _ => None,
                        })
                    }
                });
                let identical_to = g.and_then(|g| {
                    input
                        .gens
                        .iter()
                        .filter(|o| {
                            o.number < g.number && !g.target.is_empty() && o.target == g.target
                        })
                        .map(|o| o.number)
                        .min()
                });
                SysView::Generation {
                    id,
                    number: *number,
                    created: *created,
                    is_running: g.is_some_and(|g| g.is_running),
                    is_boot: g.is_some_and(|g| g.is_current),
                    built_from: links.by_gen.get(number).map(|l| CommitRef {
                        id: l.commit.clone(),
                        short: short(&l.commit),
                        how: l.how,
                    }),
                    parent_gen,
                    branched: parent_gen
                        .zip(self.newest_before(*created))
                        .is_some_and(|(p, newest)| p != newest),
                    identical_to,
                    lane,
                }
            }
            SysNode::Switch { time, to, from, .. } => SysView::Switch {
                id,
                time: *time,
                to: *to,
                from: *from,
                rollback: from.is_some_and(|f| *to < f),
                is_latest: self.latest_switch == Some(si),
                lane,
            },
        }
    }
}

/// The newest switch, when no generation came after it and its target still runs.
fn latest_switch(sys_nodes: &[SysNode], running: Option<u64>) -> Option<usize> {
    sys_nodes
        .iter()
        .position(|n| matches!(n, SysNode::Switch { .. }))
        .filter(|&i| {
            sys_nodes[..i]
                .iter()
                .all(|n| !matches!(n, SysNode::Gen { .. }))
                && matches!(&sys_nodes[i], SysNode::Switch { to, .. } if Some(*to) == running)
        })
}

/// Build the complete view: rows (newest first), lanes, links and status.
pub fn build_tree(input: &TreeInput) -> TreeView {
    let limit = input.limit.max(1);
    let commits = &input.commits;
    let links = link_generations(commits, &input.gens, &input.reflog, input.system_available);

    // System nodes newest first.
    let mut sys_nodes = system_lineage(&input.gens, &input.events);
    sys_nodes.reverse();

    let (rows, has_more) = window_rows(input, limit, &links, &sys_nodes);
    let pending = input.dirty && !input.head.is_empty();
    let offset = usize::from(pending);
    let cfg = assign_lanes(&config_dag(input, &rows, pending));
    let sys = assign_lanes(&system_dag(&rows, &sys_nodes, offset));
    let status = status_view(input, &links);
    let running_commit = status.running_commit.as_ref().map(|c| c.id.as_str());
    let sys_cx = SysCx {
        input,
        links: &links,
        sys_nodes: &sys_nodes,
        latest_switch: latest_switch(&sys_nodes, status.running),
    };

    let mut out_rows: Vec<RowView> = Vec::with_capacity(rows.len() + offset);
    if pending {
        out_rows.push(RowView {
            key: WORKTREE_ID.to_string(),
            time: i64::MAX,
            pending: true,
            pending_lane: cfg.lane_of.get(WORKTREE_ID).copied(),
            commit: None,
            system: None,
        });
    }
    for row in &rows {
        let commit = row.commit.map(|ci| {
            let c = &commits[ci];
            let lane = cfg.lane_of.get(&c.id).copied().unwrap_or(0);
            commit_view(c, input, &links, running_commit, lane)
        });
        let system = row.sys.map(|si| {
            let lane = sys.lane_of.get(&sys_nodes[si].id()).copied().unwrap_or(0);
            sys_cx.view(si, lane)
        });
        let time = commit
            .as_ref()
            .map(|c| c.timestamp)
            .or_else(|| row.sys.map(|si| sys_nodes[si].time()))
            .unwrap_or(0);
        let key = match (&commit, &system) {
            (Some(c), _) => c.id.clone(),
            (None, Some(SysView::Generation { id, .. }))
            | (None, Some(SysView::Switch { id, .. })) => id.clone(),
            _ => String::new(),
        };
        out_rows.push(RowView {
            key,
            time,
            pending: false,
            pending_lane: None,
            commit,
            system,
        });
    }

    TreeView {
        head: input.head.clone(),
        head_short: short(&input.head),
        dirty: input.dirty,
        rows: out_rows,
        config_graph: GraphView {
            lanes: cfg.lanes,
            edges: cfg.edges,
        },
        system_graph: GraphView {
            lanes: sys.lanes,
            edges: sys.edges,
        },
        system: SystemInfo {
            available: input.system_available,
            message: input.system_message.clone(),
            generations: input.gens.len(),
        },
        status,
        has_more,
        limit,
        notes: NotesView::default(),
    }
}

/// Inputs for the tests of the version tree model.
#[cfg(test)]
pub(super) mod fixtures {
    use super::{CommitIn, GenIn};

    pub fn gen(number: u64, created: i64, target: &str) -> GenIn {
        GenIn {
            number,
            created,
            target: target.to_string(),
            ..Default::default()
        }
    }

    pub fn commit(id: &str, parents: &[&str], ts: i64, generation: Option<u64>) -> CommitIn {
        CommitIn {
            id: id.to_string(),
            parents: parents.iter().map(|s| s.to_string()).collect(),
            subject: match generation {
                Some(n) => format!("Activation: activation_x (generation {n})"),
                None => "Activation: activation_x".to_string(),
            },
            timestamp: ts,
            branches: vec![format!("activation_{id}")],
            generation,
            changes: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{commit, gen};
    use super::*;

    fn scenario() -> TreeInput {
        // Config: c1 → c2 → c3 (gen 12); restore c1 → c4 (empty re-activation,
        // gen 13 identical to 11) → c5 edit (gen 14).
        // System: 11, 12, rollback to 11 at 350, then 13 and 14.
        let commits = vec![
            commit("c5", &["c4"], 705, Some(14)),
            commit("c4", &["c1"], 505, Some(13)),
            commit("c3", &["c2"], 305, Some(12)),
            commit("c2", &["c1"], 200, None),
            commit("c1", &[], 105, Some(11)),
        ];
        let mut gens = vec![
            gen(11, 100, "/a"),
            gen(12, 300, "/b"),
            gen(13, 500, "/a"),
            gen(14, 700, "/c"),
        ];
        gens[3].is_running = true;
        gens[3].is_current = true;
        TreeInput {
            head: "c5".into(),
            head_generation: Some(14),
            commits,
            system_available: true,
            gens,
            events: vec![
                (101, Some(11)),
                (301, Some(12)),
                (350, Some(11)),
                (501, Some(13)),
                (701, Some(14)),
            ],
            limit: 50,
            ..Default::default()
        }
    }

    #[test]
    fn tree_pairs_commits_with_generations() {
        let t = build_tree(&scenario());
        let keys: Vec<&str> = t.rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, vec!["c5", "c4", "s350-11", "c3", "c2", "c1"]);
        let paired: Vec<(&str, u64)> = t
            .rows
            .iter()
            .filter_map(|r| match (&r.commit, &r.system) {
                (Some(c), Some(SysView::Generation { number, .. })) => {
                    Some((c.id.as_str(), *number))
                }
                _ => None,
            })
            .collect();
        assert_eq!(paired, vec![("c5", 14), ("c4", 13), ("c3", 12), ("c1", 11)]);
        assert_eq!(t.status.kind, StatusKind::Sync);
        assert!(!t.has_more);
    }

    #[test]
    fn tree_lanes_show_both_branches() {
        let t = build_tree(&scenario());
        // Config: c3/c2 sit on a side lane that forks from c1.
        let lane = |id: &str| {
            t.rows
                .iter()
                .find_map(|r| r.commit.as_ref().filter(|c| c.id == id).map(|c| c.lane))
                .unwrap()
        };
        assert_eq!(lane("c5"), 0);
        assert_eq!(lane("c4"), 0);
        assert_eq!(lane("c3"), 1);
        assert_eq!(lane("c1"), 0);
        assert_eq!(t.config_graph.lanes, 2);
        // System: 12's line ends; the rollback node continues 11's line into 13.
        let g13 = t
            .rows
            .iter()
            .find_map(|r| match &r.system {
                Some(SysView::Generation {
                    number: 13,
                    parent_gen,
                    branched,
                    identical_to,
                    ..
                }) => Some((*parent_gen, *branched, *identical_to)),
                _ => None,
            })
            .unwrap();
        assert_eq!(g13, (Some(11), true, Some(11)));
        assert_eq!(t.system_graph.lanes, 2);
    }

    #[test]
    fn tree_detects_drift_after_rollback() {
        let mut input = scenario();
        // Roll back to 12 (built from c3) after everything.
        input.events.push((800, Some(12)));
        for g in &mut input.gens {
            g.is_running = g.number == 12;
            g.is_current = g.number == 12;
        }
        let t = build_tree(&input);
        assert_eq!(t.status.kind, StatusKind::Drift);
        assert_eq!(t.status.running, Some(12));
        assert_eq!(t.status.running_commit.as_ref().unwrap().id, "c3");
        assert_eq!(t.status.head_generation, Some(14));
        assert_eq!(t.status.running_since, Some(800));
        let c3 = t.rows.iter().find(|r| r.key == "c3").unwrap();
        assert!(c3.commit.as_ref().unwrap().is_running_config);
        // The newest row is the rollback, flagged as current position.
        match &t.rows[0].system {
            Some(SysView::Switch {
                to,
                rollback,
                is_latest,
                ..
            }) => assert_eq!((*to, *rollback, *is_latest), (12, true, true)),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn tree_identical_system_counts_as_sync() {
        let mut input = scenario();
        // Running 11 while HEAD recorded 13 — same store path.
        input.head = "c4".into();
        input.head_generation = Some(13);
        for g in &mut input.gens {
            g.is_running = g.number == 11;
        }
        let t = build_tree(&input);
        assert_eq!(t.status.kind, StatusKind::Sync);
        assert_eq!(t.status.next_boot, Some(14));
    }

    #[test]
    fn tree_unbuilt_and_pending() {
        let mut input = scenario();
        input.commits.insert(
            0,
            CommitIn {
                id: "c6".into(),
                parents: vec!["c5".into()],
                subject: "Update from neo init".into(),
                timestamp: 800,
                ..Default::default()
            },
        );
        input.head = "c6".into();
        input.head_generation = None;
        input.dirty = true;
        let t = build_tree(&input);
        assert_eq!(t.status.kind, StatusKind::Unbuilt);
        assert!(t.rows[0].pending);
        assert_eq!(t.rows[0].pending_lane, Some(0));
        assert_eq!(t.rows[1].key, "c6");
        // Worktree edge goes to HEAD's row.
        let e = t
            .config_graph
            .edges
            .iter()
            .find(|e| e.from_row == 0)
            .unwrap();
        assert_eq!(e.to_row, Some(1));
    }

    #[test]
    fn tree_window_and_sentinel() {
        let mut input = scenario();
        input.limit = 3;
        let t = build_tree(&input);
        assert_eq!(t.rows.len(), 3);
        assert!(t.has_more);
        // c4's parent (c1) is not in the window: open edge.
        let c4_row = t.rows.iter().position(|r| r.key == "c4").unwrap();
        let e = t
            .config_graph
            .edges
            .iter()
            .find(|e| e.from_row == c4_row)
            .unwrap();
        assert_eq!(e.to_row, None);

        // Sentinel: 2 commits requested (3 fetched) — rows stop before c3.
        let mut input = scenario();
        input.limit = 2;
        input.commits.truncate(3);
        let t = build_tree(&input);
        assert!(t.rows.iter().all(|r| r.key != "c3"));
        assert!(t.has_more);
    }

    #[test]
    fn tree_without_system() {
        let mut input = scenario();
        input.gens.clear();
        input.events.clear();
        input.system_available = false;
        let t = build_tree(&input);
        assert_eq!(t.status.kind, StatusKind::Unavailable);
        assert_eq!(t.rows.len(), 5);
        assert!(t.rows.iter().all(|r| r.system.is_none()));
        let c5 = t.rows[0].commit.as_ref().unwrap();
        assert_eq!(c5.gen_state, Some(CommitGenState::Unknown));
        assert_eq!(t.system_graph.lanes, 0);
    }

    #[test]
    fn json_uses_camel_case_fields() {
        let mut input = scenario();
        input.dirty = true;
        let v = serde_json::to_value(build_tree(&input)).unwrap();
        let g13 = v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["system"]["number"] == 13)
            .unwrap();
        assert_eq!(g13["system"]["type"], "generation");
        assert_eq!(g13["system"]["parentGen"], 11);
        assert_eq!(g13["system"]["builtFrom"]["how"], "recorded");
        assert_eq!(g13["commit"]["genState"], "created");
        assert_eq!(v["rows"][0]["pendingLane"], 0);
        assert!(v["configGraph"]["edges"][0].get("fromRow").is_some());
        assert_eq!(v["status"]["kind"], "sync");
    }

    #[test]
    fn commit_kinds() {
        assert_eq!(
            commit_kind("Activation: activation_x"),
            CommitKind::Activation
        );
        assert_eq!(commit_kind("Build: 2026"), CommitKind::Build);
        assert_eq!(commit_kind("Update from neo init"), CommitKind::Other);
        assert_eq!(commit_kind(""), CommitKind::Other);
    }
}
