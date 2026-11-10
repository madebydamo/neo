//! Scanners: vulnxscan (vulnix + grype + OSV over the system closure, patched
//! CVEs already dropped) and Trivy (every image a container uses).

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use super::model::{Finding, Severity};
use crate::utils::command::{env_or, stdout_checked};

pub fn docker_bin() -> String {
    env_or("DOCKER_BINARY_PATH", "docker")
}

/// The running system's store path (`/run/current-system` resolved once, so a
/// switch during the scan does not mix two closures).
pub fn current_system() -> Result<String> {
    let p = std::fs::canonicalize("/run/current-system").context("resolve /run/current-system")?;
    Ok(p.to_string_lossy().into_owned())
}

/// vulnxscan over `target`. With `--triage` it adds a repology-based class
/// per finding; repology is often slow or unreachable, so a failed triage run
/// is repeated without it.
pub fn scan_nix(target: &str, work: &Path) -> Result<(Vec<Finding>, Vec<String>)> {
    let mut notes = Vec::new();
    let out = work.join("vulns.csv");
    let triage_out = work.join("vulns.triage.csv");
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&triage_out);
    let run = |triage: bool| -> Result<()> {
        let mut cmd = Command::new("vulnxscan");
        cmd.arg("--out").arg(&out).arg("--verbose").arg("0");
        if triage {
            cmd.arg("--triage");
        }
        cmd.arg(target).current_dir(work);
        stdout_checked(&mut cmd, "vulnxscan").map(|_| ())
    };
    if let Err(e) = run(true) {
        notes.push(format!(
            "vulnxscan triage (repology) failed, scanned without it: {}",
            last_line(&format!("{e:#}"))
        ));
        run(false)?;
    }
    let path = if triage_out.exists() { triage_out } else { out };
    let csv = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    Ok((parse_vulnxscan(&csv)?, notes))
}

/// vulnxscan CSV: vuln_id, url, package, version_local, severity (CVSS),
/// grype/osv/vulnix (1 when that scanner reported it), and with --triage
/// `classify` (fix_update_to_version_nixpkgs, err_not_vulnerable_based_on_repology, …).
pub fn parse_vulnxscan(csv_text: &str) -> Result<Vec<Finding>> {
    let mut rdr = csv::Reader::from_reader(csv_text.as_bytes());
    let headers = rdr.headers().context("vulnxscan csv header")?.clone();
    let col = |name: &str| headers.iter().position(|h| h == name);
    let (Some(id), Some(pkg), Some(ver)) = (col("vuln_id"), col("package"), col("version_local"))
    else {
        bail!("vulnxscan csv: missing columns in {:?}", headers);
    };
    let url = col("url");
    let sev = col("severity");
    let classify = col("classify");
    let version_nixpkgs = col("version_nixpkgs");
    let scanners: Vec<(usize, &str)> = ["grype", "osv", "vulnix"]
        .into_iter()
        .filter_map(|s| col(s).map(|i| (i, s)))
        .collect();
    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec.context("vulnxscan csv row")?;
        let get = |i: Option<usize>| {
            i.and_then(|i| rec.get(i))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
        };
        let Some(vuln_id) = get(Some(id)) else {
            continue;
        };
        let cvss = get(sev).and_then(|s| s.parse::<f32>().ok());
        let status = get(classify);
        let fixed_version = match (status.as_deref(), get(version_nixpkgs)) {
            (Some("fix_update_to_version_nixpkgs"), Some(v)) => Some(v),
            _ => None,
        };
        out.push(Finding {
            vuln_id,
            package: get(Some(pkg)).unwrap_or_default(),
            version: get(Some(ver)).unwrap_or_default(),
            severity: cvss.map(Severity::from_cvss).unwrap_or(Severity::Unknown),
            cvss,
            fixed_version,
            status,
            title: None,
            description: None,
            url: get(url),
            scanners: scanners
                .iter()
                .filter(|(i, _)| {
                    rec.get(*i)
                        .map(str::trim)
                        .is_some_and(|v| v != "0" && !v.is_empty())
                })
                .map(|(_, s)| (*s).to_string())
                .collect(),
            pkg_class: None,
            pkg_path: None,
        });
    }
    Ok(out)
}

/// A container on this machine and the image it runs.
#[derive(Debug, Clone)]
pub struct Container {
    pub name: String,
    pub id: String,
    pub image_ref: String,
    pub image_id: String,
    pub running: bool,
}

pub fn list_containers() -> Result<Vec<Container>> {
    let ids = stdout_checked(
        Command::new(docker_bin()).args(["ps", "-aq", "--no-trunc"]),
        "docker ps",
    )?;
    let ids: Vec<&str> = ids.split_whitespace().collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let out = stdout_checked(
        Command::new(docker_bin())
            .args([
                "inspect",
                "--format",
                "{{.Name}}\t{{.Id}}\t{{.Config.Image}}\t{{.Image}}\t{{.State.Running}}",
            ])
            .args(&ids),
        "docker inspect",
    )?;
    Ok(out
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f.len() == 5).then(|| Container {
                name: f[0].trim_start_matches('/').to_string(),
                id: f[1].to_string(),
                image_ref: f[2].to_string(),
                image_id: f[3].to_string(),
                running: f[4] == "true",
            })
        })
        .collect())
}

/// Configured containers missing from `docker ps -a`, as stopped containers
/// of their image when it is in the local image store.
pub fn add_configured(containers: &mut Vec<Container>, cfg: &super::config::Config) {
    for svc in cfg.services.values() {
        for (name, image_ref) in &svc.containers {
            if containers.iter().any(|c| &c.name == name) {
                continue;
            }
            let id = stdout_checked(
                Command::new(docker_bin())
                    .args(["image", "inspect", "--format", "{{.Id}}", image_ref]),
                "docker image inspect",
            );
            if let Ok(id) = id {
                containers.push(Container {
                    name: name.clone(),
                    id: String::new(),
                    image_ref: image_ref.clone(),
                    image_id: id.trim().to_string(),
                    running: false,
                });
            }
        }
    }
}

/// Trivy over one local image (by id, from the Docker daemon, no pull).
pub fn scan_image(image_id: &str) -> Result<Vec<Finding>> {
    let mut cmd = Command::new("trivy");
    cmd.args([
        "image",
        "--quiet",
        "--format",
        "json",
        "--scanners",
        "vuln",
        "--image-src",
        "docker",
        "--timeout",
        "30m",
    ]);
    if let Ok(dir) = std::env::var("TRIVY_CACHE_DIR") {
        cmd.arg("--cache-dir").arg(dir);
    }
    cmd.arg(image_id);
    let out = stdout_checked(&mut cmd, &format!("trivy image {image_id}"))?;
    parse_trivy(&out)
}

pub fn parse_trivy(json: &str) -> Result<Vec<Finding>> {
    let v: Value = serde_json::from_str(json).context("trivy json")?;
    let mut out = Vec::new();
    for res in v["Results"].as_array().into_iter().flatten() {
        let class = res["Class"].as_str().unwrap_or("");
        let typ = res["Type"].as_str().unwrap_or("");
        let pkg_class = if class == "os-pkgs" {
            "os-pkgs".to_string()
        } else {
            typ.to_string()
        };
        let target = res["Target"].as_str().unwrap_or("");
        for vu in res["Vulnerabilities"].as_array().into_iter().flatten() {
            let s = |k: &str| {
                vu[k]
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            };
            let Some(vuln_id) = s("VulnerabilityID") else {
                continue;
            };
            let cvss = vu["CVSS"]
                .as_object()
                .and_then(|m| {
                    m.values()
                        .filter_map(|c| c["V3Score"].as_f64().or(c["V40Score"].as_f64()))
                        .reduce(f64::max)
                })
                .map(|f| f as f32);
            let pkg_path =
                s("PkgPath").or_else(|| (class != "os-pkgs").then(|| target.to_string()));
            out.push(Finding {
                vuln_id,
                package: s("PkgName").unwrap_or_default(),
                version: s("InstalledVersion").unwrap_or_default(),
                severity: Severity::parse(vu["Severity"].as_str().unwrap_or("")),
                cvss,
                fixed_version: s("FixedVersion"),
                status: s("Status"),
                title: s("Title"),
                description: s("Description").map(|d| truncate(&d, 1500)),
                url: s("PrimaryURL"),
                scanners: vec!["trivy".into()],
                pkg_class: Some(pkg_class.clone()),
                pkg_path,
            });
        }
    }
    Ok(out)
}

/// Group image ids by the containers that use them.
pub fn images_in_use(containers: &[Container]) -> BTreeMap<String, Vec<&Container>> {
    let mut m: BTreeMap<String, Vec<&Container>> = BTreeMap::new();
    for c in containers {
        m.entry(c.image_id.clone()).or_default().push(c);
    }
    m
}

pub fn first_line(s: &str) -> String {
    let l = s.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    truncate(l, 300)
}

/// The last line of a Python traceback names the actual error.
pub fn last_line(s: &str) -> String {
    let l = s.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
    truncate(l.trim(), 300)
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max).collect();
    t.push('…');
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vulnxscan_csv() {
        let csv = r#""vuln_id","url","package","version_local","severity","grype","osv","vulnix","sum","sortcol"
"CVE-2026-111497","https://nvd.nist.gov/vuln/detail/CVE-2026-111497","brotli","1.2.0","9.8","0","0","1","1","2026A0000111497"
"CVE-2026-58055","https://nvd.nist.gov/vuln/detail/CVE-2026-58055","nghttp2","1.69.0","","1","0","1","2","2026A0000058055"
"#;
        let f = parse_vulnxscan(csv).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].package, "brotli");
        assert_eq!(f[0].severity, Severity::Critical);
        assert_eq!(f[0].scanners, vec!["vulnix"]);
        assert_eq!(f[1].severity, Severity::Unknown);
        assert_eq!(f[1].scanners, vec!["grype", "vulnix"]);
    }

    #[test]
    fn parses_vulnxscan_triage_class() {
        let csv = "vuln_id,url,package,version_local,severity,version_nixpkgs,classify\nCVE-1,u,curl,8.0.1,9.1,8.9.0,fix_update_to_version_nixpkgs\n";
        let f = parse_vulnxscan(csv).unwrap();
        assert_eq!(
            f[0].status.as_deref(),
            Some("fix_update_to_version_nixpkgs")
        );
        assert_eq!(f[0].fixed_version.as_deref(), Some("8.9.0"));
    }

    #[test]
    fn parses_trivy_json() {
        let json = r#"{"Results":[
          {"Target":"x (debian 12.5)","Class":"os-pkgs","Type":"debian","Vulnerabilities":[
            {"VulnerabilityID":"CVE-2024-1","PkgName":"libssl3","InstalledVersion":"3.0.11","FixedVersion":"3.0.13","Status":"fixed","Severity":"HIGH","Title":"t","CVSS":{"nvd":{"V3Score":7.5}}}]},
          {"Target":"usr/lib/node_modules/a/package.json","Class":"lang-pkgs","Type":"node-pkg","Vulnerabilities":[
            {"VulnerabilityID":"GHSA-x","PkgName":"a","InstalledVersion":"1.0.0","Status":"affected","Severity":"CRITICAL"}]}]}"#;
        let f = parse_trivy(json).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].pkg_class.as_deref(), Some("os-pkgs"));
        assert_eq!(f[0].cvss, Some(7.5));
        assert_eq!(f[1].pkg_class.as_deref(), Some("node-pkg"));
        assert_eq!(
            f[1].pkg_path.as_deref(),
            Some("usr/lib/node_modules/a/package.json")
        );
        assert_eq!(f[1].severity, Severity::Critical);
    }
}
