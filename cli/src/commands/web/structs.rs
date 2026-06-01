use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct Service {
    pub name: String,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct IndexContext {
    pub services: Vec<Service>,
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub nix_cmd: String,
    pub neo_input: String,
    pub settings_path: PathBuf,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionType {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elem: Option<Box<OptionType>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionSchema {
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: OptionType,
    pub typeLabel: String,
    #[serde(default)]
    pub default: serde_json::Value,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<serde_json::Value>,
    #[serde(default)]
    pub internal: bool,
    #[serde(default, rename = "readOnly")]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<serde_json::Value>,
    #[serde(default)]
    pub defaultDisplay: String,
    #[serde(default)]
    pub currentDisplay: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProxiedService {
    pub name: String,
    pub subdomain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// First two letters of the service name, for overlay on icons.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub initials: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct NavigatorContext {
    pub domain: Option<String>,
    pub services: Vec<ProxiedService>,
}

/// Rich presentation metadata for a service (shown in the option pane header).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ServiceMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projectUrl: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub githubUrl: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub releaseUrl: Option<String>,
    #[serde(default)]
    pub screenshots: Vec<ServiceScreenshot>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ServiceScreenshot {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

/// Context for the per-service option pane (includes both the form fields and rich intro metadata).
#[derive(Serialize)]
pub struct OptionPaneContext {
    pub service: String,
    pub meta: Option<ServiceMeta>,
    pub options: Vec<OptionSchema>,
    /// Pre-serialized JSON for the Alpine form (the options array only).
    pub options_json: String,
}
