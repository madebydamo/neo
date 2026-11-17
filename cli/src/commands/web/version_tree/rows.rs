//! Merging the newest-first commit and system sequences into timeline rows.
use std::collections::{HashMap, HashSet};

/// One timeline row before layout: indices into the commit / system node lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowRef {
    pub commit: Option<usize>,
    pub sys: Option<usize>,
}

/// Merge newest-first commits and newest-first system nodes into rows,
/// keeping each sequence's order. `pairs` (commit index → system index) end
/// up on the same row; a pair that would force reordering (crossing) is
/// split.
pub fn merge_rows(
    commit_times: &[i64],
    sys_times: &[i64],
    pairs: &HashMap<usize, usize>,
) -> Vec<RowRef> {
    let rev: HashMap<usize, usize> = pairs.iter().map(|(c, s)| (*s, *c)).collect();
    let mut dissolved_c: HashSet<usize> = HashSet::new();
    let mut dissolved_s: HashSet<usize> = HashSet::new();
    let pair_c = |i: usize, d: &HashSet<usize>| pairs.get(&i).copied().filter(|_| !d.contains(&i));
    let pair_s = |j: usize, d: &HashSet<usize>| rev.get(&j).copied().filter(|_| !d.contains(&j));

    let (mut i, mut j) = (0usize, 0usize);
    let mut rows = Vec::new();
    while i < commit_times.len() || j < sys_times.len() {
        if i >= commit_times.len() {
            rows.push(RowRef {
                commit: None,
                sys: Some(j),
            });
            j += 1;
            continue;
        }
        if j >= sys_times.len() {
            rows.push(RowRef {
                commit: Some(i),
                sys: None,
            });
            i += 1;
            continue;
        }
        let pc = pair_c(i, &dissolved_c);
        let ps = pair_s(j, &dissolved_s);
        if pc == Some(j) {
            rows.push(RowRef {
                commit: Some(i),
                sys: Some(j),
            });
            i += 1;
            j += 1;
            continue;
        }
        // A side is blocked when its pair partner is further down the other side.
        let c_blocked = pc.is_some_and(|pj| pj > j);
        let s_blocked = ps.is_some_and(|pi| pi > i);
        let take_commit = match (c_blocked, s_blocked) {
            (true, false) => false,
            (false, true) => true,
            (true, true) => {
                // Crossing pairs: split the system node's pair, emit it alone.
                dissolved_s.insert(j);
                if let Some(pi) = ps {
                    dissolved_c.insert(pi);
                }
                false
            }
            (false, false) => commit_times[i] >= sys_times[j],
        };
        if take_commit {
            if let Some(pj) = pc {
                // Partner already emitted (cannot happen with c_blocked=false
                // unless pj < j): split.
                dissolved_c.insert(i);
                dissolved_s.insert(pj);
            }
            rows.push(RowRef {
                commit: Some(i),
                sys: None,
            });
            i += 1;
        } else {
            if let Some(pi) = pair_s(j, &dissolved_s) {
                dissolved_s.insert(j);
                dissolved_c.insert(pi);
            }
            rows.push(RowRef {
                commit: None,
                sys: Some(j),
            });
            j += 1;
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_rows_pairs_and_interleaves() {
        // commits at 500(paired s0), 300; system: 510 (paired c0), 400, 100
        let pairs = HashMap::from([(0usize, 0usize)]);
        let rows = merge_rows(&[500, 300], &[510, 400, 100], &pairs);
        assert_eq!(
            rows,
            vec![
                RowRef {
                    commit: Some(0),
                    sys: Some(0)
                },
                RowRef {
                    commit: None,
                    sys: Some(1)
                },
                RowRef {
                    commit: Some(1),
                    sys: None
                },
                RowRef {
                    commit: None,
                    sys: Some(2)
                },
            ]
        );
    }

    #[test]
    fn merge_rows_waits_for_partner() {
        // s0 (newest) pairs with c1: c0 must come first even though s0 is newer.
        let pairs = HashMap::from([(1usize, 0usize)]);
        let rows = merge_rows(&[500, 400], &[600], &pairs);
        assert_eq!(
            rows,
            vec![
                RowRef {
                    commit: Some(0),
                    sys: None
                },
                RowRef {
                    commit: Some(1),
                    sys: Some(0)
                },
            ]
        );
    }

    #[test]
    fn merge_rows_splits_crossing_pairs() {
        // c0↔s1 and c1↔s0 cannot both share rows.
        let pairs = HashMap::from([(0usize, 1usize), (1usize, 0usize)]);
        let rows = merge_rows(&[500, 400], &[500, 400], &pairs);
        assert_eq!(rows.len(), 3);
        let paired = rows
            .iter()
            .filter(|r| r.commit.is_some() && r.sys.is_some())
            .count();
        assert_eq!(paired, 1);
        // Every index appears exactly once.
        let cs: Vec<usize> = rows.iter().filter_map(|r| r.commit).collect();
        let ss: Vec<usize> = rows.iter().filter_map(|r| r.sys).collect();
        assert_eq!(cs, vec![0, 1]);
        assert_eq!(ss, vec![0, 1]);
    }
}
