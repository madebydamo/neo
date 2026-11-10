//! The operator's decisions on findings (`decisions.json` in the data dir):
//! "not relevant here" (accept) and "review again". Written by the report page
//! (as homeserver) and `neo security accept|reset`; the scan only reads them, so
//! a decision taken during a running check is never overwritten. Every read of
//! the report lays them over the verdicts ([`apply`]).

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use super::model::{key_version, package_key, Group, Report, Status, Verdict};
use crate::utils::command::now_secs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Not relevant on this server; holds for later versions of the package
    /// while the finding is still reported.
    Accept,
    /// Ignore the cached AI verdict (older than `at`) and review again.
    Review,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub kind: Kind,
    #[serde(default)]
    pub reason: String,
    pub at: u64,
}

/// Group key (origin|vuln|package|version) → decision.
pub type Decisions = BTreeMap<String, Decision>;

const FILE: &str = "decisions.json";

pub fn load(dir: &Path) -> Decisions {
    std::fs::read_to_string(dir.join(FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, d: &Decisions) -> Result<()> {
    super::write_json(&dir.join(FILE), d)
}

fn accepted(reason: &str, at: u64, note: Option<String>) -> Verdict {
    Verdict {
        status: Status::Harmless,
        confidence: "high".into(),
        summary: "You marked this as not relevant here.".into(),
        explanation: if reason.trim().is_empty() {
            "No reason given.".into()
        } else {
            reason.trim().to_string()
        },
        action: "Nothing to do.".into(),
        by: "user".into(),
        at,
        note,
    }
}

/// The accept for this finding: for its version, else the newest one for
/// another version of the package (carried over, with a note).
pub fn accept_for(d: &Decisions, g: &Group) -> Option<Verdict> {
    if let Some(x) = d.get(&g.key).filter(|x| x.kind == Kind::Accept) {
        return Some(accepted(&x.reason, x.at, None));
    }
    let prefix = package_key(g.origin, &g.vuln_id, &g.package);
    d.range(prefix.clone()..)
        .take_while(|(k, _)| k.starts_with(&prefix))
        .filter(|(_, x)| x.kind == Kind::Accept)
        .max_by_key(|(_, x)| x.at)
        .map(|(k, x)| {
            accepted(
                &x.reason,
                x.at,
                Some(format!(
                    "Accepted for version {}; still reported in {}.",
                    key_version(k),
                    g.version
                )),
            )
        })
}

/// A "review again" newer than the group's AI or rule verdict.
pub fn wants_review(d: &Decisions, g: &Group) -> bool {
    d.get(&g.key).is_some_and(|x| {
        x.kind == Kind::Review
            && g.verdict
                .as_ref()
                .is_none_or(|v| v.by != "user" && v.at <= x.at)
    })
}

/// Lay the decisions over a report's verdicts.
pub fn apply(report: &mut Report, d: &Decisions) {
    for g in &mut report.groups {
        if let Some(v) = accept_for(d, g) {
            g.verdict = Some(v);
        } else if wants_review(d, g) {
            g.verdict = None;
        }
    }
}

/// Accept (`reason`) or undo / review again (`None`) the finding `id` of the
/// last report. Returns a line for the operator.
pub fn decide(dir: &Path, id: &str, reason: Option<&str>) -> Result<String> {
    let report = super::load_report(dir)?;
    let g = report
        .groups
        .iter()
        .find(|g| g.id == id)
        .ok_or_else(|| anyhow!("no finding {id} in the last report"))?;
    let mut d = load(dir);
    let line = match reason {
        Some(reason) => {
            d.insert(
                g.key.clone(),
                Decision {
                    kind: Kind::Accept,
                    reason: reason.chars().take(500).collect(),
                    at: now_secs(),
                },
            );
            format!("{} in {} marked as not relevant here", g.vuln_id, g.package)
        }
        None => {
            // An accept of another version would still apply: drop those too.
            let prefix = package_key(g.origin, &g.vuln_id, &g.package);
            d.retain(|k, x| !(k.starts_with(&prefix) && x.kind == Kind::Accept));
            d.insert(
                g.key.clone(),
                Decision {
                    kind: Kind::Review,
                    reason: String::new(),
                    at: now_secs(),
                },
            );
            format!("{} in {} will be reviewed again", g.vuln_id, g.package)
        }
    };
    save(dir, &d).context("save decisions")?;
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::security::model::{group_key, Evidence, Origin, Severity};

    fn group(version: &str, verdict: Option<Verdict>) -> Group {
        Group {
            id: "x".into(),
            key: group_key(Origin::Image, "CVE-1", "libssl3", version),
            origin: Origin::Image,
            vuln_id: "CVE-1".into(),
            package: "libssl3".into(),
            version: version.into(),
            severity: Severity::High,
            cvss: None,
            fixed_version: None,
            status: None,
            title: None,
            description: None,
            url: None,
            scanners: vec![],
            pkg_class: None,
            pkg_paths: vec![],
            targets: vec![],
            evidence: Evidence::default(),
            verdict,
            new: false,
            previous: None,
        }
    }

    fn ai(at: u64) -> Verdict {
        Verdict {
            by: "ai:x".into(),
            at,
            ..accepted("", 0, None)
        }
    }

    #[test]
    fn accept_applies_and_carries_to_new_versions() {
        let mut d = Decisions::new();
        d.insert(
            group_key(Origin::Image, "CVE-1", "libssl3", "1.0"),
            Decision {
                kind: Kind::Accept,
                reason: "not used".into(),
                at: 5,
            },
        );
        let mut r = Report {
            groups: vec![group("1.0", Some(ai(1))), group("1.1", None)],
            ..Default::default()
        };
        apply(&mut r, &d);
        let a = r.groups[0].verdict.as_ref().unwrap();
        assert_eq!(
            (a.by.as_str(), a.explanation.as_str(), a.note.is_none()),
            ("user", "not used", true)
        );
        let b = r.groups[1].verdict.as_ref().unwrap();
        assert!(b.note.as_deref().unwrap().contains("version 1.0"));
    }

    #[test]
    fn review_again_hides_older_verdicts_only() {
        let mut d = Decisions::new();
        d.insert(
            group_key(Origin::Image, "CVE-1", "libssl3", "1.0"),
            Decision {
                kind: Kind::Review,
                reason: String::new(),
                at: 5,
            },
        );
        let mut r = Report {
            groups: vec![group("1.0", Some(ai(4)))],
            ..Default::default()
        };
        apply(&mut r, &d);
        assert!(r.groups[0].verdict.is_none());
        let mut r = Report {
            groups: vec![group("1.0", Some(ai(6)))],
            ..Default::default()
        };
        apply(&mut r, &d);
        assert!(r.groups[0].verdict.is_some());
    }
}
