//! Settings for `neo security`, written by nix/services/security/default.nix
//! as JSON (`NEO_SECURITY_CONFIG`), and the data directory (`NEO_SECURITY_DIR`).

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::model::Severity;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_floor")]
    pub min_severity: Severity,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub ai: AiConfig,
    /// report.json and verdicts.json (the service's appdata).
    pub data_dir: Option<String>,
    /// Enabled neo services: units, containers, address and exposure.
    #[serde(default)]
    pub services: BTreeMap<String, ServiceInfo>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfig {
    /// "hermes", "custom" or "off".
    pub source: String,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub key_file: Option<String>,
    pub max_reviews: usize,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            source: "off".into(),
            base_url: None,
            model: None,
            key_file: None,
            max_reviews: 40,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    #[serde(default)]
    pub units: Vec<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub ingress: Vec<String>,
    #[serde(default)]
    pub auth: bool,
    /// container name → image reference
    #[serde(default)]
    pub containers: BTreeMap<String, String>,
}

impl ServiceInfo {
    /// "web address https://x, reachable from the internet, behind the login".
    pub fn exposure(&self, name: &str) -> String {
        let Some(url) = &self.url else {
            return format!("{name}: no web address (internal only)");
        };
        let mut from: Vec<&str> = self
            .ingress
            .iter()
            .map(|i| match i.as_str() {
                "web" => "the internet",
                "tailscale" => "Tailscale",
                "netbird" => "NetBird",
                "local" => "the home network",
                other => other,
            })
            .collect();
        if from.is_empty() {
            from.push("nowhere");
        }
        format!(
            "{name}: {url}, reachable from {}, {}",
            from.join(", "),
            if self.auth {
                "behind the tinyauth login"
            } else {
                "without the tinyauth login (own login or public)"
            }
        )
    }

    pub fn on_internet(&self) -> bool {
        self.url.is_some() && self.ingress.iter().any(|i| i == "web")
    }
}

fn default_floor() -> Severity {
    Severity::High
}

fn default_theme() -> String {
    "light".into()
}

impl Config {
    /// `NEO_SECURITY_CONFIG` (the units), else `/etc/neo/security.json` (a
    /// root shell on the server), else defaults.
    pub fn load() -> Result<Self> {
        let path = std::env::var("NEO_SECURITY_CONFIG")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                Path::new(ETC_CONFIG)
                    .exists()
                    .then(|| ETC_CONFIG.to_string())
            });
        match path {
            Some(path) => {
                let text =
                    std::fs::read_to_string(&path).with_context(|| format!("read {path}"))?;
                serde_json::from_str(&text).with_context(|| format!("parse {path}"))
            }
            None => Ok(serde_json::from_str("{}")?),
        }
    }

    /// Service that owns a container (by container name).
    pub fn container_service(&self, container: &str) -> Option<&str> {
        self.services
            .iter()
            .find(|(_, s)| s.containers.contains_key(container))
            .map(|(n, _)| n.as_str())
    }
}

const ETC_CONFIG: &str = "/etc/neo/security.json";

/// report.json, verdicts.json and the scanner work files live here:
/// `NEO_SECURITY_DIR`, else the configured appdata, else ./security-data.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var("NEO_SECURITY_DIR")
        .ok()
        .filter(|s| !s.is_empty())
    {
        return PathBuf::from(d);
    }
    let configured = Config::load().ok().and_then(|c| c.data_dir);
    PathBuf::from(configured.unwrap_or_else(|| "security-data".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_has_defaults() {
        let c: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(c.min_severity, Severity::High);
        assert_eq!(c.ai.source, "off");
    }

    #[test]
    fn exposure_text() {
        let s = ServiceInfo {
            url: Some("https://a.example".into()),
            ingress: vec!["local".into(), "web".into()],
            auth: true,
            ..Default::default()
        };
        assert!(s.on_internet());
        assert_eq!(
            s.exposure("a"),
            "a: https://a.example, reachable from the home network, the internet, behind the tinyauth login"
        );
    }
}
