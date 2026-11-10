//! `neo security`: vulnerability scan of the NixOS system and the container
//! images, reviewed by an AI, shown on its own page (nix/services/security).
//!
//! scan    run the scanners, group and filter, review (root; the timer runs it)
//! triage  review the last scan again without scanning
//! show    print the last report
//! accept  mark a finding as not relevant here (kept across scans)
//! reset   forget the verdict of a finding so the next review sees it again
//! serve   the report page (neo-security-web)

pub mod ai;
pub mod chat;
pub mod config;
pub mod evidence;
pub mod model;
pub mod scan;
pub mod serve;
pub mod triage;

use anyhow::{anyhow, bail, Context, Result};
use clap::Subcommand;
use std::path::Path;

use crate::utils::command::{is_root, now_secs};
use config::{data_dir, Config};
use model::{Origin, Report, Status, Target, Verdict, Verdicts};
use triage::Located;

#[derive(Subcommand, Debug)]
pub enum SecurityAction {
    /// Scan the system and every container image, then review the findings.
    Scan {
        /// Skip the AI review (findings stay unreviewed until `triage`).
        #[arg(long)]
        no_ai: bool,
        /// Skip the NixOS system scan.
        #[arg(long)]
        no_nix: bool,
        /// Skip the container image scan.
        #[arg(long)]
        no_images: bool,
        /// Scan this store path or .drv instead of /run/current-system (the
        /// system's derivation must be in the local store).
        #[arg(long, value_name = "PATH")]
        system: Option<String>,
    },
    /// Review the last scan's findings again, without scanning.
    Triage {
        /// Also review findings that already have an AI verdict.
        #[arg(long)]
        all: bool,
    },
    /// Print the last report.
    Show {
        /// Include findings judged harmless.
        #[arg(long)]
        all: bool,
    },
    /// Mark a finding (id from `show` or the page) as not relevant on this server.
    Accept {
        id: String,
        /// Why, shown on the page.
        #[arg(long, default_value = "Accepted by the operator.")]
        reason: String,
    },
    /// Forget a finding's verdict so the next review looks at it again.
    Reset { id: String },
    /// Serve the report page (ROCKET_ADDRESS / ROCKET_PORT).
    Serve,
}

pub fn security(action: SecurityAction) -> Result<()> {
    let dir = data_dir();
    match action {
        SecurityAction::Serve => serve::serve(),
        SecurityAction::Scan {
            no_ai,
            no_nix,
            no_images,
            system,
        } => {
            if !is_root() {
                eprintln!(
                    "security: not root, so other users' processes stay out of the in-use check"
                );
            }
            scan_all(&dir, no_ai, !no_nix, !no_images, system)
        }
        SecurityAction::Triage { all } => {
            require_root()?;
            let cfg = Config::load()?;
            let mut report = load_report(&dir)?;
            let mut verdicts = load_verdicts(&dir)?;
            report.errors.retain(|e| !e.starts_with("AI review"));
            let work = work_dir(&dir)?;
            triage::review(&cfg, &mut report, &mut verdicts, all, &work, &|r, v| {
                save(&dir, r, v)
            })
        }
        SecurityAction::Show { all } => show(&load_report(&dir)?, all),
        SecurityAction::Accept { id, reason } => set_user_verdict(&dir, &id, Some(reason)),
        SecurityAction::Reset { id } => set_user_verdict(&dir, &id, None),
    }
}

/// The review reads the root-only AI key.
fn require_root() -> Result<()> {
    if !is_root() {
        bail!("run as root (the AI key is root-only), e.g. `sudo neo security triage`");
    }
    Ok(())
}

fn work_dir(dir: &Path) -> Result<std::path::PathBuf> {
    let work = dir.join("work");
    std::fs::create_dir_all(&work).with_context(|| format!("create {}", work.display()))?;
    Ok(work)
}

fn scan_all(
    dir: &Path,
    no_ai: bool,
    nix: bool,
    images: bool,
    system: Option<String>,
) -> Result<()> {
    let mut cfg = Config::load()?;
    if no_ai {
        cfg.ai.source = "off".into();
    }
    let work = work_dir(dir)?;
    let mut report = Report {
        started_at: now_secs(),
        ..Default::default()
    };
    let mut hits: Vec<Located> = Vec::new();

    if nix {
        match system.map(Ok).unwrap_or_else(scan::current_system) {
            Ok(system) => {
                eprintln!("security: vulnxscan {system}");
                match scan::scan_nix(&system, &work) {
                    Ok((findings, notes)) => {
                        report.errors.extend(notes);
                        let target = Target {
                            name: "system".into(),
                            containers: vec![],
                            services: vec![],
                        };
                        hits.extend(findings.into_iter().map(|finding| Located {
                            finding,
                            origin: Origin::Nix,
                            target: target.clone(),
                        }));
                    }
                    Err(e) => report.errors.push(format!(
                        "System scan failed: {}",
                        scan::last_line(&format!("{e:#}"))
                    )),
                }
                report.system = Some(system);
            }
            Err(e) => report.errors.push(format!("{e:#}")),
        }
    }

    let containers = if images {
        let mut cs = scan::list_containers().unwrap_or_else(|e| {
            report
                .errors
                .push(format!("Listing containers failed: {e:#}"));
            Vec::new()
        });
        // Right after boot (a missed timer run) containers may not exist yet:
        // scan the images the config names too, when they are pulled.
        scan::add_configured(&mut cs, &cfg);
        cs
    } else {
        Vec::new()
    };
    for (image_id, cs) in scan::images_in_use(&containers) {
        let image_ref = cs[0].image_ref.clone();
        eprintln!("security: trivy {image_ref}");
        match scan::scan_image(&image_id) {
            Ok(findings) => {
                let target = triage::image_target(&cfg, &image_ref, &cs);
                hits.extend(findings.into_iter().map(|finding| Located {
                    finding,
                    origin: Origin::Image,
                    target: target.clone(),
                }));
                report.images.push(image_ref);
            }
            Err(e) => report.errors.push(format!(
                "Image {image_ref} failed: {}",
                scan::first_line(&format!("{e:#}"))
            )),
        }
    }

    report.raw_findings = hits.len();
    let (mut groups, below) = triage::group(hits, cfg.min_severity);
    report.below_floor = below;
    triage::collect_evidence(&cfg, &mut groups, &containers);
    report.groups = groups;

    let mut verdicts = load_verdicts(dir)?;
    let result = triage::review(&cfg, &mut report, &mut verdicts, false, &work, &|r, v| {
        save(dir, r, v)
    });
    report.finished_at = now_secs();
    save(dir, &report, &verdicts)?;
    result?;
    eprintln!(
        "security: {} groups from {} findings ({} below the floor)",
        report.groups.len(),
        report.raw_findings,
        report.below_floor
    );
    Ok(())
}

pub fn load_report(dir: &Path) -> Result<Report> {
    let p = dir.join("report.json");
    let text =
        std::fs::read_to_string(&p).with_context(|| format!("no report yet ({})", p.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", p.display()))
}

fn load_verdicts(dir: &Path) -> Result<Verdicts> {
    let p = dir.join("verdicts.json");
    match std::fs::read_to_string(&p) {
        Ok(t) => serde_json::from_str(&t).with_context(|| format!("parse {}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Verdicts::new()),
        Err(e) => Err(e).with_context(|| format!("read {}", p.display())),
    }
}

/// Write via a temp file and rename, so the page never reads half a report.
fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))
}

fn save(dir: &Path, report: &Report, verdicts: &Verdicts) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    write_json(&dir.join("verdicts.json"), verdicts)?;
    write_json(&dir.join("report.json"), report)
}

fn set_user_verdict(dir: &Path, id: &str, reason: Option<String>) -> Result<()> {
    let mut report = load_report(dir)?;
    let mut verdicts = load_verdicts(dir)?;
    let g = report
        .groups
        .iter_mut()
        .find(|g| g.id == id)
        .ok_or_else(|| anyhow!("no finding {id} in the last report"))?;
    match reason {
        Some(reason) => {
            let v = Verdict {
                status: Status::Harmless,
                confidence: "high".into(),
                summary: "Accepted by the operator.".into(),
                explanation: reason,
                action: "Nothing to do.".into(),
                by: "user".into(),
                at: now_secs(),
            };
            verdicts.insert(g.key.clone(), v.clone());
            g.verdict = Some(v);
            println!("✓ {} in {} accepted", g.vuln_id, g.package);
        }
        None => {
            verdicts.remove(&g.key);
            g.verdict = triage::rule_verdict(g);
            println!("✓ {} in {} will be reviewed again", g.vuln_id, g.package);
        }
    }
    save(dir, &report, &verdicts)
}

fn show(report: &Report, all: bool) -> Result<()> {
    let label = |s: Option<Status>| match s {
        Some(Status::Attention) => "ATTENTION",
        Some(Status::Waiting) => "waiting",
        Some(Status::Harmless) => "harmless",
        None => "unreviewed",
    };
    println!(
        "{} groups ({} raw findings, {} below the floor), {} images, AI: {}",
        report.groups.len(),
        report.raw_findings,
        report.below_floor,
        report.images.len(),
        report.ai.as_deref().unwrap_or("off")
    );
    for e in &report.errors {
        println!("! {e}");
    }
    let mut groups: Vec<_> = report
        .groups
        .iter()
        .filter(|g| {
            all || g
                .verdict
                .as_ref()
                .is_none_or(|v| v.status != Status::Harmless)
        })
        .collect();
    groups.sort_by_key(|g| {
        (
            match g.verdict.as_ref().map(|v| v.status) {
                Some(Status::Attention) => 0,
                None => 1,
                Some(Status::Waiting) => 2,
                Some(Status::Harmless) => 3,
            },
            std::cmp::Reverse(g.severity),
        )
    });
    for g in groups {
        let place = if g.origin == Origin::Nix {
            "system".to_string()
        } else {
            g.services().join(",")
        };
        println!(
            "{}  {:<10} {:<8?} {:<20} {} {}  [{}]",
            g.id,
            label(g.verdict.as_ref().map(|v| v.status)),
            g.severity,
            g.vuln_id,
            g.package,
            g.version,
            place
        );
        if let Some(v) = &g.verdict {
            println!("              {}", v.summary);
        }
    }
    Ok(())
}
