//! Report data: findings grouped by vulnerability × package × version, the
//! evidence gathered for each group and the verdict (rule, AI or operator).
//! `report.json` and `verdicts.json` in the data dir hold these as JSON.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Unknown,
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    /// Trivy's LOW/MEDIUM/HIGH/CRITICAL; anything else is unknown.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" | "negligible" => Self::Low,
            "medium" | "moderate" => Self::Medium,
            "high" | "important" => Self::High,
            "critical" => Self::Critical,
            _ => Self::Unknown,
        }
    }

    /// CVSS base score bands (v3).
    pub fn from_cvss(score: f32) -> Self {
        match score {
            s if s >= 9.0 => Self::Critical,
            s if s >= 7.0 => Self::High,
            s if s >= 4.0 => Self::Medium,
            s if s > 0.0 => Self::Low,
            _ => Self::Unknown,
        }
    }

    /// Whether a finding of this severity stays in the report. Unknown
    /// severities count as medium: scanners leave it out for fresh CVEs.
    pub fn at_least(self, floor: Severity) -> bool {
        let s = if self == Self::Unknown {
            Self::Medium
        } else {
            self
        };
        s >= floor
    }
}

/// Where a group was found: the NixOS system closure or container images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Nix,
    Image,
}

/// One scanner hit before grouping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub vuln_id: String,
    pub package: String,
    pub version: String,
    pub severity: Severity,
    pub cvss: Option<f32>,
    pub fixed_version: Option<String>,
    /// Trivy status (fixed, affected, will_not_fix, fix_deferred, …) or the
    /// vulnxscan triage class (fix_update_to_version_nixpkgs, …).
    pub status: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub scanners: Vec<String>,
    /// Trivy: os-pkgs or the language package type (python-pkg, jar, …).
    pub pkg_class: Option<String>,
    /// Trivy: file the language package was found in.
    pub pkg_path: Option<String>,
}

/// A place a group occurs: an image (with the containers and services that
/// run it) or the system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Target {
    /// Image reference, or "system".
    pub name: String,
    pub containers: Vec<String>,
    pub services: Vec<String>,
}

/// Facts collected on the machine for one group, shown to the AI and on the page.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Evidence {
    /// Running processes that have the package's files mapped, "unit (comm)".
    pub loaded_by: Vec<String>,
    /// Whether the maps check could answer at all (container running, OS
    /// package with a file list, Nix store path match).
    pub load_checked: bool,
    /// Services involved: those running the image, or whose units have the
    /// package loaded.
    #[serde(default)]
    pub services: Vec<String>,
    /// The same services with how they are reached.
    pub exposure: Vec<String>,
    /// Plain notes from the deterministic checks.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Worth acting on now (or at least knowing about).
    Attention,
    /// Real, but only the image or nixpkgs maintainers can fix it.
    Waiting,
    /// False alarm or not reachable on this server.
    Harmless,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    pub status: Status,
    pub confidence: String,
    /// One plain sentence for the list.
    pub summary: String,
    pub explanation: String,
    pub action: String,
    /// "rule", "user" or "ai:<model>".
    pub by: String,
    pub at: u64,
    /// Shown with the verdict, e.g. an accept carried over from another version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// Stable id from `key` (for `neo security accept <id>` and anchors).
    pub id: String,
    pub key: String,
    pub origin: Origin,
    pub vuln_id: String,
    pub package: String,
    pub version: String,
    pub severity: Severity,
    pub cvss: Option<f32>,
    pub fixed_version: Option<String>,
    pub status: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub scanners: Vec<String>,
    pub pkg_class: Option<String>,
    pub pkg_paths: Vec<String>,
    pub targets: Vec<Target>,
    pub evidence: Evidence,
    pub verdict: Option<Verdict>,
    /// Not in the previous check (same vulnerability and package, any version).
    #[serde(default)]
    pub new: bool,
    /// The AI verdict for an earlier version of the package (`note` = that
    /// version), for the review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<Verdict>,
}

impl Group {
    pub fn services(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .targets
            .iter()
            .flat_map(|t| t.services.iter().cloned())
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
    pub started_at: u64,
    pub finished_at: u64,
    pub system: Option<String>,
    pub images: Vec<String>,
    /// Docker image ids scanned; with `system` they tell whether anything
    /// changed since (a check requested after an update is skipped otherwise).
    #[serde(default)]
    pub image_ids: Vec<String>,
    /// Findings left out below the severity floor.
    pub below_floor: usize,
    /// Raw scanner hits before grouping.
    pub raw_findings: usize,
    pub errors: Vec<String>,
    pub ai: Option<String>,
    pub groups: Vec<Group>,
}

/// Verdict cache: group key → verdict. Operator verdicts (`by = "user"`)
/// are never replaced by a review.
pub type Verdicts = BTreeMap<String, Verdict>;

/// FNV-1a, so ids stay the same across builds (std's hasher may change).
pub fn stable_id(key: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in key.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:012x}", h & 0xffff_ffff_ffff)
}

pub fn group_key(origin: Origin, vuln_id: &str, package: &str, version: &str) -> String {
    format!("{}{version}", package_key(origin, vuln_id, package))
}

/// The key without the version: the same finding in any version of the package.
pub fn package_key(origin: Origin, vuln_id: &str, package: &str) -> String {
    let o = match origin {
        Origin::Nix => "nix",
        Origin::Image => "image",
    };
    format!("{o}|{vuln_id}|{package}|")
}

/// The package version a key (and its cached verdict) is for.
pub fn key_version(key: &str) -> &str {
    key.rsplit_once('|').map_or("", |(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_bands_and_floor() {
        assert_eq!(Severity::from_cvss(9.8), Severity::Critical);
        assert_eq!(Severity::from_cvss(7.0), Severity::High);
        assert_eq!(Severity::from_cvss(0.0), Severity::Unknown);
        assert_eq!(Severity::parse("HIGH"), Severity::High);
        assert!(Severity::Unknown.at_least(Severity::Medium));
        assert!(!Severity::Unknown.at_least(Severity::High));
        assert!(Severity::Critical.at_least(Severity::High));
    }

    #[test]
    fn stable_ids_do_not_change() {
        let k = group_key(Origin::Nix, "CVE-2024-1", "curl", "8.0.1");
        assert_eq!(stable_id(&k), stable_id(&k));
        assert_eq!(stable_id(&k).len(), 12);
        assert_ne!(
            stable_id(&k),
            stable_id(&group_key(Origin::Image, "CVE-2024-1", "curl", "8.0.1"))
        );
    }
}
