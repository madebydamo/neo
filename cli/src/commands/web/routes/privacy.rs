//! Privacy map: `/configuration/privacy` (neo.privacy.categories × services × guides).
use std::sync::Arc;

use rocket::{get, State};
use rocket_dyn_templates::{Metadata, Template};
use serde::{Deserialize, Serialize};

use super::pages::configuration_shell;
use crate::commands::web::nix::errors::eval_error_ui;
use crate::commands::web::types::{AppConfig, EvalErrorUi};
use crate::commands::web::util::Htmx;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PrivacyService {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub tagline: String,
    #[serde(default)]
    pub replaces: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PrivacyGuide {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub minutes: Option<u32>,
    #[serde(default)]
    pub difficulty: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PrivacyCategory {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub icon_tint: bool,
    #[serde(default)]
    pub cloud: String,
    #[serde(default)]
    pub home: String,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub services: Vec<PrivacyService>,
    #[serde(default)]
    pub guides: Vec<PrivacyGuide>,
    #[serde(default)]
    pub home_now: bool,
    /// Position on the orbit look (percent of the square scene) and float delay.
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub delay: f32,
}

/// Orbit radii (percent of the scene): data at home circles close to the
/// server core, data still in the cloud drifts on the outer ring.
const HOME_RING: f32 = 23.0;
const CLOUD_RING: f32 = 41.0;

/// Spread each group evenly around its ring, starting at the top.
fn place(at_home: &mut [PrivacyCategory], in_cloud: &mut [PrivacyCategory]) {
    fn ring(items: &mut [PrivacyCategory], r: f32, offset: f32) {
        let n = items.len().max(1) as f32;
        for (i, c) in items.iter_mut().enumerate() {
            let a = (i as f32 + offset) / n * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            c.x = 50.0 + r * a.cos();
            c.y = 50.0 + r * a.sin();
            c.delay = (i as f32 * 0.7) % 5.0;
        }
    }
    ring(at_home, HOME_RING, 0.0);
    ring(in_cloud, CLOUD_RING, 0.5);
}

/// The easiest kind of data to bring home next: a cloud category with the
/// shortest guide, else the first one with an app for it.
#[derive(Serialize, Default)]
struct NextStep {
    id: String,
    label: String,
    icon: Option<String>,
    icon_tint: bool,
    guide: Option<String>,
    minutes: Option<u32>,
    app: Option<String>,
}

fn next_step(in_cloud: &[PrivacyCategory]) -> Option<NextStep> {
    let quickest = in_cloud
        .iter()
        .filter_map(|c| {
            let g = c
                .guides
                .iter()
                .min_by_key(|g| g.minutes.unwrap_or(u32::MAX))?;
            Some((c, g))
        })
        .min_by_key(|(_, g)| g.minutes.unwrap_or(u32::MAX));
    let (c, guide) = match quickest {
        Some((c, g)) => (c, Some(g)),
        None => (in_cloud.iter().find(|c| !c.services.is_empty())?, None),
    };
    Some(NextStep {
        id: c.id.clone(),
        label: c.label.clone(),
        icon: c.icon.clone(),
        icon_tint: c.icon_tint,
        guide: guide.map(|g| g.title.clone()),
        minutes: guide.and_then(|g| g.minutes),
        app: c.services.first().map(|s| s.name.clone()),
    })
}

#[derive(Deserialize, Default)]
pub struct PrivacyExtract {
    #[serde(default)]
    pub categories: Vec<PrivacyCategory>,
    /// False when the operator turned guides off (neo.services.neo.guides).
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Default)]
struct PrivacyContext {
    at_home: Vec<PrivacyCategory>,
    in_cloud: Vec<PrivacyCategory>,
    all: Vec<PrivacyCategory>,
    home_count: usize,
    cloud_count: usize,
    total: usize,
    /// Share kept at home, 0–100 (progress ring and meter).
    percent: u32,
    next: Option<NextStep>,
    assistant: bool,
    disabled: bool,
    #[serde(flatten)]
    eval_error: EvalErrorUi,
}

async fn context(config: &AppConfig) -> PrivacyContext {
    let mut ev = config.evaluator.lock().await;
    match ev.extract_privacy().await {
        Ok(x) => {
            let mut at_home: Vec<_> = x
                .categories
                .iter()
                .filter(|c| c.home_now)
                .cloned()
                .collect();
            let mut in_cloud: Vec<_> = x
                .categories
                .iter()
                .filter(|c| !c.home_now)
                .cloned()
                .collect();
            place(&mut at_home, &mut in_cloud);
            let all: Vec<_> = at_home.iter().chain(in_cloud.iter()).cloned().collect();
            let total = all.len();
            PrivacyContext {
                home_count: at_home.len(),
                cloud_count: in_cloud.len(),
                total,
                percent: (at_home.len() * 100).checked_div(total).unwrap_or(0) as u32,
                next: next_step(&in_cloud),
                at_home,
                in_cloud,
                all,
                assistant: crate::commands::web::agent::available(),
                disabled: !x.enabled,
                ..Default::default()
            }
        }
        Err(e) => {
            eprintln!("web: nix extract_privacy failed: {e:#}");
            PrivacyContext {
                eval_error: eval_error_ui("Nix error while building the privacy map", &e),
                ..Default::default()
            }
        }
    }
}

#[get("/configuration/privacy")]
pub async fn configuration_privacy(
    config: &State<Arc<AppConfig>>,
    htmx: Htmx,
    meta: Metadata<'_>,
) -> Template {
    let ctx = context(config).await;
    if htmx.is_htmx() {
        return Template::render("privacy", ctx);
    }
    let html = meta.render("privacy", &ctx).map(|(_, h)| h);
    configuration_shell(
        config,
        "/configuration/privacy",
        "guides",
        Some("Privacy map".into()),
        html,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cats(n: usize) -> Vec<PrivacyCategory> {
        (0..n)
            .map(|i| PrivacyCategory {
                id: format!("c{i}"),
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn rings_keep_home_inside_and_cloud_outside() {
        for n in 0..=16 {
            let mut home = cats(n);
            let mut cloud = cats(16 - n);
            place(&mut home, &mut cloud);
            let dist = |c: &PrivacyCategory| ((c.x - 50.0).powi(2) + (c.y - 50.0).powi(2)).sqrt();
            for c in &home {
                assert!((dist(c) - HOME_RING).abs() < 0.01, "{n}: {} {}", c.x, c.y);
            }
            for c in &cloud {
                assert!((dist(c) - CLOUD_RING).abs() < 0.01, "{n}: {} {}", c.x, c.y);
                assert!((0.0..=100.0).contains(&c.x) && (0.0..=100.0).contains(&c.y));
            }
        }
    }

    #[test]
    fn next_step_prefers_the_shortest_guide() {
        let mut c = cats(3);
        c[0].services = vec![PrivacyService {
            name: "a".into(),
            ..Default::default()
        }];
        c[1].guides = vec![PrivacyGuide {
            id: "slow".into(),
            minutes: Some(40),
            ..Default::default()
        }];
        c[2].guides = vec![PrivacyGuide {
            id: "quick".into(),
            title: "Quick".into(),
            minutes: Some(10),
            ..Default::default()
        }];
        let n = next_step(&c).expect("next");
        assert_eq!(n.id, "c2");
        assert_eq!(n.guide.as_deref(), Some("Quick"));
        let n = next_step(&c[..1]).expect("app only");
        assert_eq!(n.app.as_deref(), Some("a"));
        assert!(next_step(&cats(2)).is_none());
    }
}
