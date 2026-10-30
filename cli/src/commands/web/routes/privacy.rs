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
    /// Position in the map scene (percent of width / height) and float delay.
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub delay: f32,
}

/// Sky positions (x %, y %), clear of the house (x 30–70 %, y 34–100 %).
const SKY: &[(f32, f32)] = &[
    (8.0, 11.0),
    (21.0, 23.0),
    (34.0, 9.0),
    (48.0, 21.0),
    (62.0, 8.0),
    (76.0, 21.0),
    (91.0, 11.0),
    (10.0, 38.0),
    (90.0, 38.0),
    (19.0, 55.0),
    (81.0, 55.0),
    (8.0, 72.0),
    (92.0, 72.0),
    (21.0, 88.0),
    (79.0, 88.0),
    (40.0, 30.0),
    (60.0, 30.0),
    (50.0, 5.0),
];

/// Spread `n` items over the sky slots and the house body (x 36–64 %, y 58–90 %).
fn place(at_home: &mut [PrivacyCategory], in_cloud: &mut [PrivacyCategory]) {
    for (i, c) in in_cloud.iter_mut().enumerate() {
        let (x, y) = SKY[i % SKY.len()];
        // A second lap shifts slightly so icons do not stack exactly.
        let lap = (i / SKY.len()) as f32;
        c.x = x + lap * 4.0;
        c.y = y + lap * 5.0;
        c.delay = (i as f32 * 0.7) % 5.0;
    }
    let n = at_home.len();
    if n == 0 {
        return;
    }
    let cols = n.clamp(1, 3);
    let rows = n.div_ceil(cols);
    for (i, c) in at_home.iter_mut().enumerate() {
        let (r, col) = (i / cols, i % cols);
        let in_row = if r == rows - 1 { n - r * cols } else { cols };
        let step_x = 28.0 / in_row as f32;
        c.x = 36.0 + step_x * (col as f32 + 0.5);
        c.y = 58.0 + (32.0 / rows as f32) * (r as f32 + 0.5);
        c.delay = (i as f32 * 0.9) % 4.0;
    }
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
    total: usize,
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
            PrivacyContext {
                home_count: at_home.len(),
                total: all.len(),
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
    fn home_icons_stay_inside_the_house_and_sky_icons_outside() {
        for n in 0..=16 {
            let mut home = cats(n);
            let mut cloud = cats(16 - n);
            place(&mut home, &mut cloud);
            for c in &home {
                assert!(
                    (36.0..=64.0).contains(&c.x) && (58.0..=90.0).contains(&c.y),
                    "{n}: {} {}",
                    c.x,
                    c.y
                );
            }
            for c in &cloud {
                let in_house = (30.0..=70.0).contains(&c.x) && c.y >= 34.0;
                assert!(!in_house, "{n}: sky icon in house at {} {}", c.x, c.y);
            }
        }
    }
}
