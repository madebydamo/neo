//! The security report's half of the chat drawer: the routes, the drawer and
//! the `@` popup are neo web's (`web::chat`, static/agent_chat*.js). Contexts:
//! `security:general` (the whole report) and `security:<finding id>`; `@` tags
//! findings by CVE id.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{json, Value};

use super::config::data_dir;
use super::load_report;
use super::model::{Group, Origin, Report, Status};
use crate::commands::web::agent_refs::{self, RefItem};
use crate::commands::web::chat::{BoxFut, ChatHost};
use crate::commands::web::guides::{render, RenderCtx};

const GENERAL: &str = "security:general";

const STYLE: &str = "You are answering in the chat of the neo security report, a page listing known vulnerabilities an automatic scan found on this home server (the NixOS system and the Docker images), each with a verdict from an AI review. The person may not be technical: use plain words and short steps. Help them understand a finding, whether it matters on this server and what they can do. Check facts read-only (versions, files, /proc, docker inspect) and change nothing unless the user asks for it. Fixes usually come from upstream: a newer image (`neo docker-update <container>`) or a system update (`neo update`, then `neo activate`), once the maintainers ship one. On the server, `neo security show` lists the findings, `neo security accept <id> --reason \"…\"` marks one as not relevant here (kept across scans), `neo security reset <id>` has it reviewed again, `systemctl start neo-security-scan` runs a new check. Never show a password, token or key (write *** instead).";

/// The chat host of the security report page.
pub struct SecurityHost;

fn finding_id_ok(id: &str) -> bool {
    id.len() == 12 && id.chars().all(|c| c.is_ascii_hexdigit())
}

fn report() -> Option<Report> {
    load_report(&data_dir()).ok()
}

fn status_word(g: &Group) -> &'static str {
    match g.verdict.as_ref().map(|v| v.status) {
        Some(Status::Attention) => "needs attention",
        Some(Status::Waiting) => "waiting for a fix",
        Some(Status::Harmless) => "harmless here",
        None => "not reviewed",
    }
}

fn place(g: &Group) -> String {
    let services = g.services();
    match (g.origin, services.is_empty()) {
        (Origin::Nix, _) => "NixOS system".into(),
        (Origin::Image, true) => g
            .targets
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        (Origin::Image, false) => services.join(", "),
    }
}

/// What a referenced or opened finding is, for the assistant.
fn finding_context(g: &Group) -> Value {
    json!({
        "kind": "finding",
        "id": g.id,
        "vulnerability": g.vuln_id,
        "package": g.package,
        "version": g.version,
        "found_in": match g.origin { Origin::Nix => "NixOS system", Origin::Image => "Docker image" },
        "severity": g.severity,
        "cvss": g.cvss,
        "fixed_version": g.fixed_version,
        "scanner_status": g.status,
        "scanners": g.scanners,
        "title": g.title,
        "description": g.description,
        "url": g.url,
        "where": g.targets,
        "evidence": g.evidence,
        "verdict": g.verdict,
        "link": format!("#f-{}", g.id),
    })
}

/// Token characters the chat's `@` parser accepts (`agent_refs::parse_refs`).
fn token_part(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    t.trim_end_matches(['.', '-']).to_string()
}

/// One `@` token per finding: the CVE id when only one finding has it,
/// else `<CVE>:<package>`, else with the finding id.
fn tokens(r: &Report) -> Vec<(String, &Group)> {
    let mut by_vuln: HashMap<&str, usize> = HashMap::new();
    for g in &r.groups {
        *by_vuln.entry(g.vuln_id.as_str()).or_default() += 1;
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for g in &r.groups {
        let vuln = token_part(&g.vuln_id);
        let mut t = if by_vuln[g.vuln_id.as_str()] == 1 {
            vuln.clone()
        } else {
            format!("{vuln}:{}", token_part(&g.package))
        };
        if seen.contains(&t) {
            t = format!("{vuln}:{}", g.id);
        }
        seen.insert(t.clone());
        out.push((t, g));
    }
    out
}

fn rank(g: &Group) -> (u8, std::cmp::Reverse<super::model::Severity>) {
    let s = match g.verdict.as_ref().map(|v| v.status) {
        Some(Status::Attention) => 0,
        None => 1,
        Some(Status::Waiting) => 2,
        Some(Status::Harmless) => 3,
    };
    (s, std::cmp::Reverse(g.severity))
}

fn candidates(r: &Report) -> Vec<RefItem> {
    let mut list = tokens(r);
    list.sort_by_key(|(_, g)| rank(g));
    list.into_iter()
        .map(|(token, g)| RefItem {
            kind: "finding",
            label: format!("{} · {} {}", g.vuln_id, g.package, g.version),
            hint: format!("{} · {}", place(g), status_word(g)),
            icon: None,
            enabled: true,
            url: format!("#f-{}", g.id),
            advanced: g
                .verdict
                .as_ref()
                .is_some_and(|v| v.status == Status::Harmless),
            secret: false,
            token,
        })
        .collect()
}

fn summary(r: &Report) -> Value {
    let count = |s: Option<Status>| {
        r.groups
            .iter()
            .filter(|g| g.verdict.as_ref().map(|v| v.status) == s)
            .count()
    };
    let mut open: Vec<&Group> = r
        .groups
        .iter()
        .filter(|g| {
            g.verdict
                .as_ref()
                .is_none_or(|v| v.status != Status::Harmless)
        })
        .collect();
    open.sort_by_key(|g| rank(g));
    json!({
        "findings": r.groups.len(),
        "needs_attention": count(Some(Status::Attention)),
        "waiting_for_fix": count(Some(Status::Waiting)),
        "not_reviewed": count(None),
        "harmless": count(Some(Status::Harmless)),
        "images": r.images,
        "errors": r.errors,
        "top_open": open.iter().take(15).map(|g| json!({
            "id": g.id, "vulnerability": g.vuln_id, "package": g.package, "version": g.version,
            "severity": g.severity, "where": place(g), "status": status_word(g),
            "summary": g.verdict.as_ref().map(|v| v.summary.clone()),
        })).collect::<Vec<_>>(),
    })
}

impl ChatHost for SecurityHost {
    fn context(&self, raw: &str) -> Option<String> {
        match raw {
            "" | "general" | "security" | GENERAL => Some(GENERAL.to_string()),
            _ => raw
                .strip_prefix("security:")
                .filter(|id| finding_id_ok(id))
                .map(|id| format!("security:{id}")),
        }
    }

    fn owns(&self, context: &str) -> bool {
        context.starts_with("security:")
    }

    fn labels(&self) -> BoxFut<'_, HashMap<String, (String, Option<String>)>> {
        Box::pin(async {
            let mut out = HashMap::new();
            out.insert(GENERAL.to_string(), ("Security".to_string(), None));
            if let Some(r) = report() {
                for g in &r.groups {
                    out.insert(
                        format!("security:{}", g.id),
                        (format!("{} in {}", g.vuln_id, g.package), None),
                    );
                }
            }
            out
        })
    }

    fn instructions<'a>(&'a self, context: &'a str) -> BoxFut<'a, String> {
        Box::pin(async move {
            let r = report();
            let id = context.strip_prefix("security:").unwrap_or("general");
            let finding = r
                .as_ref()
                .and_then(|r| r.groups.iter().find(|g| g.id == id));
            match (finding, &r) {
                (Some(g), _) => format!(
                    "{STYLE}\n\nThe user opened the chat from this finding (id {}): {}",
                    g.id,
                    finding_context(g)
                ),
                (None, Some(r)) => format!(
                    "{STYLE}\n\nThe user opened the chat from the report. The last check: {}",
                    summary(r)
                ),
                (None, None) => format!("{STYLE}\n\nNo check has run yet."),
            }
        })
    }

    fn refs(&self) -> BoxFut<'_, Result<Arc<Vec<RefItem>>, String>> {
        Box::pin(async {
            Ok(Arc::new(
                report().map(|r| candidates(&r)).unwrap_or_default(),
            ))
        })
    }

    fn ref_block<'a>(&'a self, message: &'a str) -> BoxFut<'a, Option<String>> {
        Box::pin(async move {
            let wanted = agent_refs::parse_refs(message);
            if wanted.is_empty() {
                return None;
            }
            let r = report()?;
            let toks = tokens(&r);
            let items: Vec<Value> = wanted
                .iter()
                .filter_map(|w| toks.iter().find(|(t, _)| t == w))
                .map(|(_, g)| finding_context(g))
                .collect();
            agent_refs::block(&items)
        })
    }

    fn render<'a>(&'a self, text: &'a str) -> BoxFut<'a, String> {
        Box::pin(async move {
            let icons = HashMap::new();
            render(
                text,
                &RenderCtx {
                    broken: &[],
                    service_icons: &icons,
                    assistant: false,
                },
            )
            .0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::security::model::{Evidence, Severity, Target};

    fn g(id: &str, vuln: &str, pkg: &str) -> Group {
        Group {
            id: id.into(),
            key: format!("image|{vuln}|{pkg}|1"),
            origin: Origin::Image,
            vuln_id: vuln.into(),
            package: pkg.into(),
            version: "1".into(),
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
            targets: vec![Target {
                name: "img".into(),
                containers: vec![],
                services: vec![],
            }],
            evidence: Evidence::default(),
            verdict: None,
        }
    }

    #[test]
    fn tokens_are_short_unique_and_parseable() {
        let r = Report {
            groups: vec![
                g("aaaaaaaaaaaa", "CVE-2026-1", "libbrotli1"),
                g("bbbbbbbbbbbb", "CVE-2026-1", "brotli+x"),
                g("cccccccccccc", "CVE-2026-2", "zlib"),
            ],
            ..Default::default()
        };
        let t: Vec<String> = tokens(&r).into_iter().map(|(t, _)| t).collect();
        assert_eq!(
            t,
            ["CVE-2026-1:libbrotli1", "CVE-2026-1:brotli-x", "CVE-2026-2"]
        );
        for tok in &t {
            assert_eq!(
                agent_refs::parse_refs(&format!("about @{tok}?")),
                [tok.clone()]
            );
        }
    }

    #[test]
    fn contexts() {
        let h = SecurityHost;
        assert_eq!(h.context("").as_deref(), Some(GENERAL));
        assert_eq!(
            h.context("security:0123456789ab").as_deref(),
            Some("security:0123456789ab")
        );
        assert_eq!(h.context("security:../x"), None);
        assert_eq!(h.context("service:pihole"), None);
        assert!(h.owns("security:general"));
        assert!(!h.owns("general"));
    }
}
