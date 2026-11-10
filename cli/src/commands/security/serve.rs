//! The report page: read-only, renders report.json with neo's own CSS
//! (`/static/neo-ui.css`) so it looks like the neo web UI. Served on loopback
//! behind SWAG + tinyauth; changes go through the `neo security` CLI. With
//! Hermes on, neo web's chat drawer and its routes (`web::chat`) are mounted
//! too, with this page's contexts and `@` findings (`chat.rs`).

use anyhow::{Context, Result};
use rocket::fs::FileServer;
use rocket::http::ContentType;
use rocket::http::Status as HttpStatus;
use rocket::serde::json::Json;
use rocket::{get, post, routes};
use rocket_dyn_templates::Template;
use serde::Serialize;

use super::config::{data_dir, Config};
use super::load_report;
use super::model::{Group, Origin, Report, Severity, Status};
use crate::commands::web::{agent, chat};
use crate::utils::command::env_or;

pub fn serve() -> Result<()> {
    let rt = tokio::runtime::Runtime::new().context("create runtime")?;
    rt.block_on(async {
        rocket::build()
            .mount("/static", FileServer::from(env_or("STATIC_DIR", "static")))
            .mount(
                "/",
                routes![
                    index,
                    report_json,
                    health,
                    status,
                    request_scan,
                    accept,
                    reset
                ],
            )
            .mount("/", chat::routes())
            .manage::<chat::Host>(std::sync::Arc::new(super::chat::SecurityHost))
            .attach(Template::fairing())
            .configure(
                rocket::Config::figment()
                    .merge(("template_dir", env_or("TEMPLATE_DIR", "templates"))),
            )
            .launch()
            .await
            .map_err(|e| anyhow::anyhow!("rocket launch failed: {e}"))?;
        Ok(())
    })
}

#[get("/health")]
fn health() -> &'static str {
    "ok"
}

/// Where the Check now button leaves a request; `neo-security-scan.path`
/// starts the (root) scan when it appears, so this server needs no rights.
fn request_file() -> String {
    env_or("NEO_SECURITY_REQUEST", "/run/neo-security/scan-request")
}

/// The scan unit is running (a oneshot is `activating` while it works).
fn scan_running() -> bool {
    std::process::Command::new("systemctl")
        .args(["is-active", "neo-security-scan.service"])
        .output()
        .is_ok_and(|o| {
            matches!(
                String::from_utf8_lossy(&o.stdout).trim(),
                "active" | "activating"
            )
        })
}

fn scan_requested() -> bool {
    std::path::Path::new(&request_file()).exists()
}

#[derive(Serialize)]
struct ScanState {
    running: bool,
    requested: bool,
    finished_at: u64,
}

fn scan_state() -> ScanState {
    ScanState {
        running: scan_running(),
        requested: scan_requested(),
        finished_at: load_report(&data_dir()).map_or(0, |r| r.finished_at),
    }
}

#[get("/status")]
fn status() -> Json<ScanState> {
    Json(scan_state())
}

#[derive(serde::Deserialize)]
struct ScanReq {}

/// Check now. JSON only, so a cross-site form cannot trigger it.
#[post("/scan", format = "json", data = "<_req>")]
fn request_scan(_req: Json<ScanReq>) -> Result<Json<ScanState>, (HttpStatus, String)> {
    if !scan_running() {
        std::fs::write(request_file(), "manual\n").map_err(|e| {
            (
                HttpStatus::InternalServerError,
                format!("could not request a check: {e}"),
            )
        })?;
    }
    Ok(Json(scan_state()))
}

/// One writer at a time for decisions.json.
static DECIDE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn decide(id: &str, reason: Option<&str>) -> Result<Json<serde_json::Value>, (HttpStatus, String)> {
    if !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err((HttpStatus::BadRequest, "unknown finding".into()));
    }
    let _guard = DECIDE.lock().unwrap_or_else(|e| e.into_inner());
    let line = super::decisions::decide(&data_dir(), id, reason)
        .map_err(|e| (HttpStatus::BadRequest, format!("{e:#}")))?;
    Ok(Json(serde_json::json!({ "ok": true, "message": line })))
}

#[derive(serde::Deserialize)]
struct AcceptReq {
    #[serde(default)]
    reason: String,
}

/// Not relevant here (kept across checks and later versions).
#[post("/findings/<id>/accept", format = "json", data = "<req>")]
fn accept(id: &str, req: Json<AcceptReq>) -> Result<Json<serde_json::Value>, (HttpStatus, String)> {
    decide(id, Some(&req.reason))
}

/// Undo an accept or review again: queues a review-only run (after a running
/// check, through the same request file as Check now).
#[post("/findings/<id>/reset", format = "json", data = "<_req>")]
fn reset(id: &str, _req: Json<ScanReq>) -> Result<Json<serde_json::Value>, (HttpStatus, String)> {
    let out = decide(id, None)?;
    if !scan_requested() {
        std::fs::write(request_file(), "triage\n").map_err(|e| {
            (
                HttpStatus::InternalServerError,
                format!("could not queue the review: {e}"),
            )
        })?;
    }
    Ok(out)
}

#[get("/report.json")]
fn report_json() -> (ContentType, String) {
    let body =
        std::fs::read_to_string(data_dir().join("report.json")).unwrap_or_else(|_| "{}".into());
    (ContentType::JSON, body)
}

#[get("/")]
fn index() -> Template {
    let cfg = Config::load().ok();
    let theme = cfg
        .as_ref()
        .map(|c| c.theme.clone())
        .unwrap_or_else(|| "light".into());
    let floor = cfg
        .as_ref()
        .map(|c| c.min_severity)
        .unwrap_or(Severity::High);
    let view = match load_report(&data_dir()) {
        Ok(r) => page(
            &r,
            theme,
            floor,
            cfg.as_ref().map_or(40, |c| c.ai.max_reviews),
        ),
        Err(_) => Page {
            theme,
            empty: true,
            floor: severity_label(floor),
            ..Default::default()
        },
    };
    let st = scan_state();
    let view = Page {
        assistant: agent::available(),
        scanning: st.running || st.requested,
        ..view
    };
    Template::render("security/report", view)
}

#[derive(Serialize, Default)]
struct Page {
    theme: String,
    /// Hermes is reachable: show the chat drawer and its buttons.
    assistant: bool,
    empty: bool,
    /// Report still being written (reviews in progress).
    running: bool,
    /// The scan unit runs or a check was requested.
    scanning: bool,
    started_at: u64,
    finished_at: u64,
    ai: Option<String>,
    max_reviews: usize,
    floor: &'static str,
    images: usize,
    system: bool,
    raw_findings: usize,
    group_count: usize,
    below_floor: usize,
    errors: Vec<String>,
    attention: usize,
    waiting: usize,
    harmless: usize,
    unreviewed: usize,
    new_count: usize,
    groups: Vec<Row>,
}

#[derive(Serialize)]
struct Row {
    id: String,
    status: &'static str,
    status_label: &'static str,
    status_class: &'static str,
    severity: &'static str,
    severity_class: &'static str,
    cvss: Option<String>,
    vuln_id: String,
    url: Option<String>,
    package: String,
    version: String,
    fixed_version: Option<String>,
    origin: &'static str,
    places: Vec<Place>,
    place_summary: String,
    title: Option<String>,
    description: Option<String>,
    summary: Option<String>,
    explanation: Option<String>,
    action: Option<String>,
    by: Option<String>,
    confidence: Option<String>,
    /// E.g. an accept carried over from an earlier version.
    note: Option<String>,
    /// You marked it as not relevant (shows Undo instead of the accept form).
    accepted: bool,
    /// An AI or rule verdict that can be reviewed again.
    reviewable: bool,
    /// Not in the previous check.
    new: bool,
    loaded: String,
    loaded_class: &'static str,
    exposure: Vec<String>,
    notes: Vec<String>,
    scanners: String,
    search: String,
}

#[derive(Serialize)]
struct Place {
    name: String,
    detail: String,
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Critical => "Critical",
        Severity::High => "High",
        Severity::Medium => "Medium",
        Severity::Low => "Low",
        Severity::Unknown => "Unknown",
    }
}

fn row(g: &Group) -> Row {
    let status = g.verdict.as_ref().map(|v| v.status);
    let (status, status_label, status_class) = match status {
        Some(Status::Attention) => ("attention", "Needs attention", "badge-error"),
        Some(Status::Waiting) => ("waiting", "Waiting for a fix", "badge-warning"),
        Some(Status::Harmless) => ("harmless", "Harmless here", "badge-success"),
        None => ("unreviewed", "Not reviewed", "badge-ghost"),
    };
    let severity_class = match g.severity {
        Severity::Critical => "badge-error",
        Severity::High => "badge-warning",
        Severity::Medium => "badge-info",
        _ => "badge-ghost",
    };
    let places: Vec<Place> = match g.origin {
        Origin::Nix => vec![Place {
            name: "NixOS system".into(),
            detail: if g.evidence.services.is_empty() {
                String::new()
            } else {
                g.evidence.services.join(", ")
            },
        }],
        Origin::Image => g
            .targets
            .iter()
            .map(|t| Place {
                name: t.name.clone(),
                detail: if t.services.is_empty() {
                    t.containers.join(", ")
                } else {
                    format!("{} · {}", t.services.join(", "), t.containers.join(", "))
                },
            })
            .collect(),
    };
    let services: Vec<String> = g
        .services()
        .into_iter()
        .chain(g.evidence.services.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let place_summary = match (g.origin, services.is_empty()) {
        (Origin::Nix, true) => "System".into(),
        (Origin::Nix, false) => format!("System · {}", services.join(", ")),
        (Origin::Image, true) => format!(
            "{} image{}",
            g.targets.len(),
            if g.targets.len() == 1 { "" } else { "s" }
        ),
        (Origin::Image, false) => services.join(", "),
    };
    let (loaded, loaded_class) = if !g.evidence.loaded_by.is_empty() {
        (
            format!("Loaded by {}", g.evidence.loaded_by.join(", ")),
            "text-error",
        )
    } else if g.evidence.load_checked {
        (
            "Not loaded by any running process".to_string(),
            "text-success",
        )
    } else {
        (
            "Could not check whether it is loaded".to_string(),
            "text-base-content/60",
        )
    };
    let v = g.verdict.as_ref();
    Row {
        id: g.id.clone(),
        status,
        status_label,
        status_class,
        severity: severity_label(g.severity),
        severity_class,
        cvss: g.cvss.map(|c| format!("{c:.1}")),
        vuln_id: g.vuln_id.clone(),
        url: g.url.clone(),
        package: g.package.clone(),
        version: g.version.clone(),
        fixed_version: g.fixed_version.clone(),
        origin: match g.origin {
            Origin::Nix => "System",
            Origin::Image => "Containers",
        },
        search: format!(
            "{} {} {} {}",
            g.vuln_id,
            g.package,
            place_summary,
            g.title.as_deref().unwrap_or("")
        )
        .to_lowercase(),
        places,
        place_summary,
        title: g.title.clone(),
        description: g.description.clone(),
        summary: v.map(|v| v.summary.clone()),
        explanation: v.map(|v| v.explanation.clone()),
        action: v.map(|v| v.action.clone()),
        by: v.map(|v| match v.by.as_str() {
            "rule" => "Automatic rule".to_string(),
            "user" => "You".to_string(),
            other => other
                .strip_prefix("ai:")
                .map(|m| format!("AI · {m}"))
                .unwrap_or_else(|| other.to_string()),
        }),
        confidence: v
            .filter(|v| v.by.starts_with("ai:"))
            .map(|v| v.confidence.clone()),
        note: v.and_then(|v| v.note.clone()),
        accepted: v.is_some_and(|v| v.by == "user"),
        reviewable: v.is_some_and(|v| v.by != "user"),
        new: g.new,
        loaded,
        loaded_class,
        exposure: g.evidence.exposure.clone(),
        notes: g.evidence.notes.clone(),
        scanners: g.scanners.join(", "),
    }
}

fn page(r: &Report, theme: String, floor: Severity, max_reviews: usize) -> Page {
    let mut groups: Vec<&Group> = r.groups.iter().collect();
    groups.sort_by_key(|g| {
        (
            match g.verdict.as_ref().map(|v| v.status) {
                Some(Status::Attention) => 0,
                None => 1,
                Some(Status::Waiting) => 2,
                Some(Status::Harmless) => 3,
            },
            std::cmp::Reverse(g.severity),
            g.vuln_id.clone(),
        )
    });
    let count = |s: Option<Status>| {
        r.groups
            .iter()
            .filter(|g| g.verdict.as_ref().map(|v| v.status) == s)
            .count()
    };
    Page {
        theme,
        assistant: false,
        empty: false,
        running: r.finished_at == 0,
        scanning: false,
        started_at: r.started_at,
        finished_at: r.finished_at,
        ai: r.ai.clone(),
        max_reviews,
        floor: severity_label(floor),
        images: r.images.len(),
        system: r.system.is_some(),
        raw_findings: r.raw_findings,
        group_count: r.groups.len(),
        below_floor: r.below_floor,
        errors: r.errors.clone(),
        attention: count(Some(Status::Attention)),
        waiting: count(Some(Status::Waiting)),
        harmless: count(Some(Status::Harmless)),
        unreviewed: count(None),
        new_count: r.groups.iter().filter(|g| g.new).count(),
        groups: groups.into_iter().map(row).collect(),
    }
}
