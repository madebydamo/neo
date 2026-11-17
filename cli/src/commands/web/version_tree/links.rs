//! Commit ↔ generation links: which config commit a generation was built
//! from, recorded by `neo activate` in the commit message (`(generation N)`),
//! or inferred from the HEAD reflog for generations built outside Neo's
//! activate.
use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::view::{CommitIn, GenIn};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkHow {
    /// `neo activate` wrote the generation into the commit message.
    Recorded,
    /// Config HEAD when the generation was built (reflog / commit time).
    Inferred,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenLink {
    pub commit: String,
    pub how: LinkHow,
}

/// How a commit relates to the generation recorded on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommitGenState {
    /// This activation built the generation.
    Created,
    /// Re-activation that produced an existing generation (system unchanged).
    Same,
    /// The generation has been deleted (garbage collected).
    Missing,
    /// Generations are not visible on this machine.
    Unknown,
}

#[derive(Clone, Debug, Default)]
pub struct Links {
    /// generation → commit it was built from
    pub by_gen: HashMap<u64, GenLink>,
    /// commit → (recorded generation, relation)
    pub by_commit: HashMap<String, (u64, CommitGenState)>,
}

/// Link generations to config commits.
///
/// Recorded: commits whose message names generation N. Several commits can
/// name the same N when a re-activation produced an identical system; the one
/// closest in time to the generation's creation built it, the rest are
/// "same". Unrecorded generations are inferred from the HEAD reflog (the
/// commit HEAD pointed at when the generation was created), else from the
/// newest commit older than the generation.
pub fn link_generations(
    commits: &[CommitIn],
    gens: &[GenIn],
    reflog: &[(i64, String)],
    system_available: bool,
) -> Links {
    let mut links = Links::default();
    let gen_by_n: HashMap<u64, &GenIn> = gens.iter().map(|g| (g.number, g)).collect();

    let mut claims: HashMap<u64, Vec<&CommitIn>> = HashMap::new();
    for c in commits {
        if let Some(n) = c.generation {
            claims.entry(n).or_default().push(c);
        }
    }
    for (n, cs) in &claims {
        let Some(g) = gen_by_n.get(n) else {
            let state = if system_available {
                CommitGenState::Missing
            } else {
                CommitGenState::Unknown
            };
            for c in cs {
                links.by_commit.insert(c.id.clone(), (*n, state));
            }
            continue;
        };
        let builder = cs
            .iter()
            .min_by_key(|c| ((c.timestamp - g.created).abs(), c.timestamp))
            .map(|c| c.id.clone())
            .unwrap_or_default();
        for c in cs {
            let state = if c.id == builder {
                CommitGenState::Created
            } else {
                CommitGenState::Same
            };
            links.by_commit.insert(c.id.clone(), (*n, state));
        }
        links.by_gen.insert(
            *n,
            GenLink {
                commit: builder,
                how: LinkHow::Recorded,
            },
        );
    }

    let known: HashSet<&str> = commits.iter().map(|c| c.id.as_str()).collect();
    let mut reflog: Vec<&(i64, String)> = reflog.iter().collect();
    reflog.sort_by_key(|(t, _)| *t);
    for g in gens {
        if links.by_gen.contains_key(&g.number) {
            continue;
        }
        let from_reflog = reflog
            .iter()
            .rev()
            .find(|(t, _)| *t <= g.created)
            .map(|(_, id)| id.as_str())
            .filter(|id| known.contains(id));
        let from_time = || {
            commits
                .iter()
                .filter(|c| c.timestamp <= g.created)
                .max_by_key(|c| c.timestamp)
                .map(|c| c.id.as_str())
        };
        if let Some(id) = from_reflog.or_else(from_time) {
            links.by_gen.insert(
                g.number,
                GenLink {
                    commit: id.to_string(),
                    how: LinkHow::Inferred,
                },
            );
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::super::view::fixtures::{commit, gen};
    use super::*;

    #[test]
    fn recorded_links_pick_builder_and_mark_same() {
        let commits = vec![
            commit("c3", &["c2"], 610, Some(12)), // re-activation, identical system
            commit("c2", &["c1"], 305, Some(12)),
            commit("c1", &[], 105, Some(11)),
        ];
        let gens = vec![gen(11, 100, "/a"), gen(12, 300, "/b")];
        let l = link_generations(&commits, &gens, &[], true);
        assert_eq!(
            l.by_gen[&12],
            GenLink {
                commit: "c2".into(),
                how: LinkHow::Recorded
            }
        );
        assert_eq!(l.by_commit["c2"], (12, CommitGenState::Created));
        assert_eq!(l.by_commit["c3"], (12, CommitGenState::Same));
        assert_eq!(l.by_commit["c1"], (11, CommitGenState::Created));
    }

    #[test]
    fn missing_generation_is_flagged() {
        let commits = vec![commit("c1", &[], 105, Some(7))];
        let l = link_generations(&commits, &[gen(11, 100, "/a")], &[], true);
        assert_eq!(l.by_commit["c1"], (7, CommitGenState::Missing));
        let l = link_generations(&commits, &[], &[], false);
        assert_eq!(l.by_commit["c1"], (7, CommitGenState::Unknown));
    }

    #[test]
    fn unrecorded_generation_inferred_from_reflog_then_time() {
        let commits = vec![
            commit("c2", &["c1"], 300, Some(2)),
            commit("c1", &[], 100, None),
        ];
        let gens = vec![gen(1, 150, "/a"), gen(2, 305, "/b"), gen(3, 400, "/c")];
        // HEAD was on c1 until 290, then c2.
        let reflog = vec![(90, "c1".to_string()), (290, "gone".to_string())];
        let l = link_generations(&commits, &gens, &reflog, true);
        assert_eq!(
            l.by_gen[&1],
            GenLink {
                commit: "c1".into(),
                how: LinkHow::Inferred
            }
        );
        // Reflog points at an amended-away commit → fall back to commit time.
        assert_eq!(l.by_gen[&3].commit, "c2");
        assert_eq!(l.by_gen[&3].how, LinkHow::Inferred);
        assert_eq!(l.by_gen[&2].how, LinkHow::Recorded);
    }

    #[test]
    fn generation_older_than_all_commits_is_unlinked() {
        let commits = vec![commit("c1", &[], 500, None)];
        let l = link_generations(&commits, &[gen(1, 100, "/a")], &[], true);
        assert!(!l.by_gen.contains_key(&1));
    }
}
