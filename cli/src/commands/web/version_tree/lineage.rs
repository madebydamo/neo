//! System lineage: generations and activation records as a tree. A
//! generation's parent is whatever was running when it was built, and a
//! switch back to an older generation (rollback) becomes its own node, so a
//! build after a rollback branches off the generation that was rolled back to.
use std::collections::{HashMap, HashSet};

use super::view::GenIn;

/// Seconds between a generation's creation and its first switch that still
/// count as one activation (building can take a while after the link exists
/// only in odd setups; the switch normally follows within seconds).
const FIRST_SWITCH_SLACK: i64 = 6 * 3600;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SysNode {
    Gen {
        number: u64,
        created: i64,
        parent: Option<String>,
    },
    /// Running system changed to an existing generation that is not a fresh
    /// build: rollback (older) or roll-forward (newer, ran before).
    Switch {
        time: i64,
        to: u64,
        from: Option<u64>,
        parent: String,
    },
}

impl SysNode {
    pub fn id(&self) -> String {
        match self {
            SysNode::Gen { number, .. } => gen_id(*number),
            SysNode::Switch { time, to, .. } => format!("s{time}-{to}"),
        }
    }
    pub fn time(&self) -> i64 {
        match self {
            SysNode::Gen { created, .. } => *created,
            SysNode::Switch { time, .. } => *time,
        }
    }
    pub fn parent(&self) -> Option<&String> {
        match self {
            SysNode::Gen { parent, .. } => parent.as_ref(),
            SysNode::Switch { parent, .. } => Some(parent),
        }
    }
}

pub fn gen_id(n: u64) -> String {
    format!("g{n}")
}

/// Generations and switches, oldest first, each linked to its lineage parent.
///
/// Before the first activation record exists, every new generation is assumed
/// to have been switched to when it was built (linear history, like
/// [`crate::utils::generation::GenerationTimeline::at`]).
pub fn system_lineage(gens: &[GenIn], events: &[(i64, Option<u64>)]) -> Vec<SysNode> {
    #[derive(Clone, Copy)]
    enum Ev {
        Created(u64),
        Ran(Option<u64>),
    }
    let first_record = events.iter().map(|(t, _)| *t).min();
    let mut timeline: Vec<(i64, u8, Ev)> = Vec::new();
    for g in gens {
        // Creation sorts before a switch at the same second.
        timeline.push((g.created, 0, Ev::Created(g.number)));
    }
    for (t, g) in events {
        timeline.push((*t, 1, Ev::Ran(*g)));
    }
    timeline.sort_by_key(|(t, o, ev)| {
        (
            *t,
            *o,
            match ev {
                Ev::Created(n) => *n,
                Ev::Ran(g) => g.unwrap_or(0),
            },
        )
    });
    let known: HashSet<u64> = gens.iter().map(|g| g.number).collect();
    let created_at: HashMap<u64, i64> = gens.iter().map(|g| (g.number, g.created)).collect();

    let mut out = Vec::new();
    let mut running_node: Option<String> = None;
    let mut running_gen: Option<u64> = None;
    let mut ran_before: HashSet<u64> = HashSet::new();

    for (t, _, ev) in timeline {
        match ev {
            Ev::Created(n) => {
                out.push(SysNode::Gen {
                    number: n,
                    created: t,
                    parent: running_node.clone(),
                });
                if first_record.is_none_or(|f| t < f) {
                    running_node = Some(gen_id(n));
                    running_gen = Some(n);
                    ran_before.insert(n);
                }
            }
            Ev::Ran(None) => {
                running_node = None;
                running_gen = None;
            }
            Ev::Ran(Some(g)) => {
                if running_gen == Some(g) || !known.contains(&g) {
                    continue;
                }
                let fresh = !ran_before.contains(&g)
                    && running_gen.is_none_or(|r| g > r)
                    && created_at
                        .get(&g)
                        .is_some_and(|c| t - c <= FIRST_SWITCH_SLACK);
                if fresh {
                    running_node = Some(gen_id(g));
                } else {
                    let node = SysNode::Switch {
                        time: t,
                        to: g,
                        from: running_gen,
                        parent: gen_id(g),
                    };
                    running_node = Some(node.id());
                    out.push(node);
                }
                running_gen = Some(g);
                ran_before.insert(g);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::view::fixtures::gen;
    use super::*;

    fn parent_of(nodes: &[SysNode], id: &str) -> Option<String> {
        nodes
            .iter()
            .find(|n| n.id() == id)
            .and_then(|n| n.parent().cloned())
    }

    #[test]
    fn lineage_linear_without_records() {
        let gens = vec![gen(1, 100, "/a"), gen(2, 200, "/b"), gen(3, 300, "/c")];
        let l = system_lineage(&gens, &[]);
        assert_eq!(l.len(), 3);
        assert_eq!(parent_of(&l, "g1"), None);
        assert_eq!(parent_of(&l, "g2"), Some("g1".into()));
        assert_eq!(parent_of(&l, "g3"), Some("g2".into()));
    }

    #[test]
    fn lineage_rollback_creates_switch_node_and_branch() {
        // 10 → 11 → 12 built and switched; rollback to 10; build 13.
        let gens = vec![
            gen(10, 100, "/a"),
            gen(11, 200, "/b"),
            gen(12, 300, "/c"),
            gen(13, 500, "/d"),
        ];
        let events = vec![
            (101, Some(10)),
            (201, Some(11)),
            (301, Some(12)),
            (400, Some(10)),
            (501, Some(13)),
        ];
        let l = system_lineage(&gens, &events);
        let switch = l
            .iter()
            .find(|n| matches!(n, SysNode::Switch { .. }))
            .unwrap();
        assert_eq!(
            switch,
            &SysNode::Switch {
                time: 400,
                to: 10,
                from: Some(12),
                parent: "g10".into()
            }
        );
        assert_eq!(parent_of(&l, "g13"), Some(switch.id()));
        assert_eq!(parent_of(&l, "g12"), Some("g11".into()));
        assert_eq!(parent_of(&l, "g11"), Some("g10".into()));
        // Only one switch node: fresh builds are not switches.
        assert_eq!(
            l.iter()
                .filter(|n| matches!(n, SysNode::Switch { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn lineage_ignores_reboots_into_same_generation() {
        let gens = vec![gen(1, 100, "/a"), gen(2, 200, "/b")];
        let events = vec![
            (101, Some(1)),
            (201, Some(2)),
            (900, Some(2)),
            (950, Some(2)),
        ];
        let l = system_lineage(&gens, &events);
        assert_eq!(l.len(), 2);
        assert_eq!(parent_of(&l, "g2"), Some("g1".into()));
    }

    #[test]
    fn lineage_roll_forward_is_a_switch() {
        let gens = vec![gen(1, 100, "/a"), gen(2, 200, "/b")];
        let events = vec![
            (101, Some(1)),
            (201, Some(2)),
            (300, Some(1)),
            (400, Some(2)),
        ];
        let l = system_lineage(&gens, &events);
        let switches: Vec<(u64, Option<u64>)> = l
            .iter()
            .filter_map(|n| match n {
                SysNode::Switch { to, from, .. } => Some((*to, *from)),
                _ => None,
            })
            .collect();
        assert_eq!(switches, vec![(1, Some(2)), (2, Some(1))]);
    }

    #[test]
    fn lineage_boot_mode_first_run_is_not_a_switch() {
        // Gen 2 built for next boot (no switch), running at the reboot later.
        let gens = vec![gen(1, 100, "/a"), gen(2, 200, "/b")];
        let events = vec![(101, Some(1)), (3000, Some(2))];
        let l = system_lineage(&gens, &events);
        assert_eq!(l.len(), 2);
    }

    #[test]
    fn lineage_records_start_mid_history() {
        // Records only begin at 250: 1 and 2 are linear by creation time.
        let gens = vec![gen(1, 100, "/a"), gen(2, 200, "/b"), gen(3, 300, "/c")];
        let events = vec![(250, Some(1)), (301, Some(3))];
        let l = system_lineage(&gens, &events);
        assert_eq!(parent_of(&l, "g2"), Some("g1".into()));
        // At 250 the system was switched back to 1 → switch node, 3 builds on it.
        let s = l
            .iter()
            .find(|n| matches!(n, SysNode::Switch { .. }))
            .unwrap();
        assert_eq!(parent_of(&l, "g3"), Some(s.id()));
    }
}
