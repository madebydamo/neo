//! From scanner hits to the report: severity floor, grouping across images,
//! evidence, deterministic rules, cached verdicts, then the AI review of
//! what is still open.

use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use super::ai::{osv_record, Client};
use super::config::Config;
use super::evidence::{image_load, unit_owners, ProcIndex};
use super::model::{
    group_key, stable_id, Evidence, Finding, Group, Origin, Report, Severity, Status, Target,
    Verdict, Verdicts,
};
use super::scan::{first_line, Container};
use crate::utils::command::now_secs;

/// A scanner hit and where it was found.
pub struct Located {
    pub finding: Finding,
    pub origin: Origin,
    pub target: Target,
}

/// Merge hits with the same vulnerability, package and version (one per image
/// layer stack that shares them), dropping those below the floor.
pub fn group(hits: Vec<Located>, floor: Severity) -> (Vec<Group>, usize) {
    let mut below = 0;
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    for h in hits {
        let f = h.finding;
        if !f.severity.at_least(floor) {
            below += 1;
            continue;
        }
        let key = group_key(h.origin, &f.vuln_id, &f.package, &f.version);
        let g = groups.entry(key.clone()).or_insert_with(|| Group {
            id: stable_id(&key),
            key,
            origin: h.origin,
            vuln_id: f.vuln_id.clone(),
            package: f.package.clone(),
            version: f.version.clone(),
            severity: f.severity,
            cvss: f.cvss,
            fixed_version: f.fixed_version.clone(),
            status: f.status.clone(),
            title: f.title.clone(),
            description: f.description.clone(),
            url: f.url.clone(),
            scanners: Vec::new(),
            pkg_class: f.pkg_class.clone(),
            pkg_paths: Vec::new(),
            targets: Vec::new(),
            evidence: Evidence::default(),
            verdict: None,
        });
        g.severity = g.severity.max(f.severity);
        g.cvss = match (g.cvss, f.cvss) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        g.fixed_version = g.fixed_version.take().or(f.fixed_version);
        g.title = g.title.take().or(f.title);
        g.description = g.description.take().or(f.description);
        for s in f.scanners {
            if !g.scanners.contains(&s) {
                g.scanners.push(s);
            }
        }
        if let Some(p) = f.pkg_path {
            if !g.pkg_paths.contains(&p) {
                g.pkg_paths.push(p);
            }
        }
        if !g.targets.contains(&h.target) {
            g.targets.push(h.target);
        }
    }
    let mut out: Vec<Group> = groups.into_values().collect();
    for g in &mut out {
        g.targets.sort();
    }
    (out, below)
}

/// The target for an image: its reference, containers and their services.
pub fn image_target(cfg: &Config, image_ref: &str, containers: &[&Container]) -> Target {
    let mut services: BTreeSet<String> = BTreeSet::new();
    for c in containers {
        if let Some(s) = cfg.container_service(&c.name) {
            services.insert(s.to_string());
        }
    }
    Target {
        name: image_ref.to_string(),
        containers: containers.iter().map(|c| c.name.clone()).collect(),
        services: services.into_iter().collect(),
    }
}

/// Loaded-by, exposure and notes for every group.
pub fn collect_evidence(cfg: &Config, groups: &mut [Group], containers: &[Container]) {
    let procs = ProcIndex::scan();
    let owners = unit_owners(&cfg.services);
    let ids: HashMap<&str, &str> = containers
        .iter()
        .map(|c| (c.name.as_str(), c.id.as_str()))
        .collect();
    let mut file_cache = HashMap::new();
    for g in groups.iter_mut() {
        let mut ev = Evidence::default();
        let mut services: BTreeSet<String> = g.services().into_iter().collect();
        match g.origin {
            Origin::Nix => {
                let users = procs.nix_users(&g.package, &g.version);
                ev.load_checked = true;
                for p in &users {
                    ev.loaded_by.push(p.label());
                    if let Some(s) = p.unit.as_ref().and_then(|u| owners.get(u)) {
                        services.insert(s.clone());
                    }
                }
                if g.scanners == ["vulnix"] {
                    ev.notes.push(
                        "Only vulnix reported this. vulnix matches package names against NVD product names, which often mixes up unrelated projects with the same name.".into(),
                    );
                }
                match g.status.as_deref() {
                    Some("fix_update_to_version_nixpkgs") => ev.notes.push(format!(
                        "Repology: nixpkgs unstable has a fixed version{}.",
                        g.fixed_version
                            .as_deref()
                            .map(|v| format!(" ({v})"))
                            .unwrap_or_default()
                    )),
                    Some("fix_update_to_version_upstream") => ev.notes.push(
                        "Repology: upstream has a fixed version that nixpkgs does not ship yet."
                            .into(),
                    ),
                    Some("fix_not_available") => ev
                        .notes
                        .push("Repology: no fixed version is known yet.".into()),
                    _ => {}
                }
            }
            Origin::Image => {
                let cs: Vec<(String, String)> = g
                    .targets
                    .iter()
                    .flat_map(|t| t.containers.iter())
                    .filter_map(|n| ids.get(n.as_str()).map(|id| (n.clone(), (*id).to_string())))
                    .collect();
                let os_pkg = g.pkg_class.as_deref() == Some("os-pkgs");
                let load = image_load(
                    &procs,
                    &cs,
                    &g.package,
                    os_pkg,
                    &g.pkg_paths,
                    &mut file_cache,
                );
                ev.loaded_by = load.loaded_by.into_iter().collect();
                ev.load_checked = load.checked;
                if !os_pkg {
                    ev.notes.push(
                        "Language package: only native modules show up in memory maps, so 'not loaded' proves little.".into(),
                    );
                }
                match g.status.as_deref() {
                    Some("will_not_fix") => ev
                        .notes
                        .push("The image's distribution decided not to fix this (will_not_fix).".into()),
                    Some("fix_deferred") => ev
                        .notes
                        .push("The image's distribution postponed the fix (fix_deferred).".into()),
                    Some("fixed") => ev.notes.push(
                        "A fixed package exists; a newer image release would include it once the maintainers rebuild.".into(),
                    ),
                    _ => {}
                }
            }
        }
        for s in &services {
            if let Some(info) = cfg.services.get(s) {
                ev.exposure.push(info.exposure(s));
            }
        }
        ev.services = services.into_iter().collect();
        g.evidence = ev;
    }
}

/// Verdicts that need no AI.
pub fn rule_verdict(g: &Group) -> Option<Verdict> {
    let v = |status, summary: &str, explanation: &str, action: &str| Verdict {
        status,
        confidence: "high".into(),
        summary: summary.into(),
        explanation: explanation.into(),
        action: action.into(),
        by: "rule".into(),
        at: now_secs(),
    };
    match (g.origin, g.status.as_deref()) {
        (Origin::Nix, Some("err_not_vulnerable_based_on_repology")) => Some(v(
            Status::Harmless,
            "This version is not affected.",
            "Repology's vulnerability data does not list this package version as affected, so the scanner's match is a false alarm.",
            "Nothing to do.",
        )),
        (Origin::Image, Some("will_not_fix")) => Some(v(
            Status::Waiting,
            "The image's operating system will not fix this.",
            "The distribution the image is built on rated this as minor and decided not to ship a fix. Only a different base image would remove it.",
            "Nothing to do unless the app's maintainers change their base image.",
        )),
        _ => None,
    }
}

fn exposed_on_internet(cfg: &Config, g: &Group) -> bool {
    g.services()
        .iter()
        .chain(&g.evidence.services)
        .any(|s| cfg.services.get(s).is_some_and(|i| i.on_internet()))
}

/// Critical findings in an internet-facing service are never dismissed on a
/// guess: a harmless verdict needs high confidence.
pub fn guard(cfg: &Config, g: &Group, mut v: Verdict) -> Verdict {
    if v.status == Status::Harmless
        && v.confidence != "high"
        && g.severity == Severity::Critical
        && exposed_on_internet(cfg, g)
    {
        v.status = Status::Attention;
        v.explanation.push_str(" (Kept for review: critical, in a service reachable from the internet, and the AI was not sure.)");
    }
    v
}

/// Order of review: most severe, internet-facing and loaded first.
fn review_order(cfg: &Config, g: &Group) -> (std::cmp::Reverse<Severity>, bool, bool) {
    (
        std::cmp::Reverse(g.severity),
        !exposed_on_internet(cfg, g),
        g.evidence.loaded_by.is_empty(),
    )
}

/// Findings of the same package, version and places go to the AI together
/// (its evidence is the same), at most `BATCH` per request, in review order.
const BATCH: usize = 8;

fn batches(groups: &[Group], order: &[usize]) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut slot: HashMap<String, usize> = HashMap::new();
    for &i in order {
        let g = &groups[i];
        let places: Vec<&str> = g.targets.iter().map(|t| t.name.as_str()).collect();
        let key = format!(
            "{:?}|{}|{}|{}",
            g.origin,
            g.package,
            g.version,
            places.join(",")
        );
        match slot.get(&key) {
            Some(&b) if out[b].len() < BATCH => out[b].push(i),
            _ => {
                slot.insert(key, out.len());
                out.push(vec![i]);
            }
        }
    }
    out
}

/// Rules and cached verdicts, then the AI for open groups (up to the budget).
/// Saves the report after every review so the page shows progress.
pub fn review(
    cfg: &Config,
    report: &mut Report,
    verdicts: &mut Verdicts,
    redo_ai: bool,
    dir: &Path,
    save: &dyn Fn(&Report, &Verdicts) -> Result<()>,
) -> Result<()> {
    for g in report.groups.iter_mut() {
        let cached = verdicts.get(&g.key).filter(|v| v.by == "user" || !redo_ai);
        g.verdict = match cached {
            Some(v) if v.by == "user" => Some(v.clone()),
            _ => rule_verdict(g).or_else(|| cached.cloned()),
        };
    }
    let client = match Client::from_config(&cfg.ai) {
        Ok(c) => c,
        Err(e) => {
            report.errors.push(format!("AI review: {e:#}"));
            None
        }
    };
    report.ai = client.as_ref().map(Client::label);
    save(report, verdicts)?;
    let Some(client) = client else {
        return Ok(());
    };
    let mut open: Vec<usize> = (0..report.groups.len())
        .filter(|&i| report.groups[i].verdict.is_none())
        .collect();
    open.sort_by_key(|&i| review_order(cfg, &report.groups[i]));
    let batches = batches(&report.groups, &open);
    let total = batches.len();
    let mut failures = 0;
    for (n, batch) in batches.into_iter().take(cfg.ai.max_reviews).enumerate() {
        let first = &report.groups[batch[0]];
        eprintln!(
            "security: review {}/{} {} {} ({} finding{})",
            n + 1,
            total,
            first.package,
            first.version,
            batch.len(),
            if batch.len() == 1 { "" } else { "s" }
        );
        let input: Vec<(&Group, Option<serde_json::Value>)> = batch
            .iter()
            .map(|&i| (&report.groups[i], osv_record(&report.groups[i].vuln_id)))
            .collect();
        match client.review(&input, dir) {
            Ok(answers) => {
                for &i in &batch {
                    let g = &report.groups[i];
                    // A single unnamed verdict answers a batch of one.
                    let v = answers
                        .iter()
                        .find(|(id, _)| id.eq_ignore_ascii_case(&g.vuln_id))
                        .or_else(|| (batch.len() == 1).then(|| &answers[0]))
                        .map(|(_, v)| guard(cfg, g, v.clone()));
                    if let Some(v) = v {
                        verdicts.insert(g.key.clone(), v.clone());
                        report.groups[i].verdict = Some(v);
                    }
                }
                save(report, verdicts)?;
            }
            Err(e) => {
                failures += 1;
                eprintln!("security: review failed: {e:#}");
                if failures == 3 {
                    report.errors.push(format!(
                        "AI review stopped after 3 failures: {}",
                        first_line(&format!("{e:#}"))
                    ));
                    break;
                }
            }
        }
    }
    save(report, verdicts).context("save report")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::security::config::ServiceInfo;

    fn hit(id: &str, pkg: &str, sev: Severity, image: &str) -> Located {
        Located {
            finding: Finding {
                vuln_id: id.into(),
                package: pkg.into(),
                version: "1.0".into(),
                severity: sev,
                cvss: None,
                fixed_version: None,
                status: None,
                title: None,
                description: None,
                url: None,
                scanners: vec!["trivy".into()],
                pkg_class: Some("os-pkgs".into()),
                pkg_path: None,
            },
            origin: Origin::Image,
            target: Target {
                name: image.into(),
                containers: vec![format!("{image}-c")],
                services: vec!["web".into()],
            },
        }
    }

    #[test]
    fn groups_across_images_and_drops_below_floor() {
        let (groups, below) = group(
            vec![
                hit("CVE-1", "libssl3", Severity::High, "a"),
                hit("CVE-1", "libssl3", Severity::High, "b"),
                hit("CVE-2", "zlib", Severity::Low, "a"),
            ],
            Severity::High,
        );
        assert_eq!(below, 1);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].targets.len(), 2);
    }

    #[test]
    fn guard_keeps_unsure_critical_internet_findings() {
        let mut cfg: Config = serde_json::from_str("{}").unwrap();
        cfg.services.insert(
            "web".into(),
            ServiceInfo {
                url: Some("https://x".into()),
                ingress: vec!["web".into()],
                ..Default::default()
            },
        );
        let (groups, _) = group(
            vec![hit("CVE-1", "p", Severity::Critical, "a")],
            Severity::Low,
        );
        let v = Verdict {
            status: Status::Harmless,
            confidence: "medium".into(),
            summary: String::new(),
            explanation: String::new(),
            action: String::new(),
            by: "ai:x".into(),
            at: 0,
        };
        assert_eq!(guard(&cfg, &groups[0], v.clone()).status, Status::Attention);
        let sure = Verdict {
            confidence: "high".into(),
            ..v
        };
        assert_eq!(guard(&cfg, &groups[0], sure).status, Status::Harmless);
    }

    #[test]
    fn batches_by_package_and_place() {
        let (groups, _) = group(
            vec![
                hit("CVE-1", "libssl3", Severity::High, "a"),
                hit("CVE-2", "libssl3", Severity::High, "a"),
                hit("CVE-3", "libssl3", Severity::High, "b"),
                hit("CVE-4", "zlib", Severity::High, "a"),
            ],
            Severity::Low,
        );
        let order: Vec<usize> = (0..groups.len()).collect();
        let b = batches(&groups, &order);
        assert_eq!(b.len(), 3);
        assert_eq!(b.iter().map(Vec::len).max(), Some(2));
    }

    #[test]
    fn will_not_fix_needs_no_ai() {
        let mut h = hit("CVE-1", "p", Severity::High, "a");
        h.finding.status = Some("will_not_fix".into());
        let (groups, _) = group(vec![h], Severity::Low);
        assert_eq!(rule_verdict(&groups[0]).unwrap().status, Status::Waiting);
    }
}
