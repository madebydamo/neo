use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as AsyncMutex;

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Service {
    pub name: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<i64>,
}

#[derive(Serialize, Default)]
pub struct IndexContext {
    pub services: Vec<Service>,
    #[serde(default)]
    pub theme: String,
    /// If set, the nix evaluator hit an error/timeout (e.g. flake eval taking >10min or broken flake.lock referencing missing store paths). The UI shows this message instead of hanging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
}

#[derive(Serialize)]
pub struct BranchesContext {
    pub graph: String,
    pub branches: Vec<BranchInfo>,
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub nix_cmd: String,
    pub neo_input: String,
    pub settings_path: PathBuf,
    pub evaluator: Arc<AsyncMutex<super::nix::NixEvaluator>>,
    /// Shared with the persistent nix repl: true while an evaluation/refresh is in flight.
    pub eval_busy: Arc<AtomicBool>,
    /// Sender for broadcasting HTML OOB swap fragments over WS to htmx clients
    /// (unit controls + action-bar status + container pull progress).
    pub unit_updates: tokio::sync::broadcast::Sender<String>,
    /// Systemd unit names with an in-flight docker pull+restart (dedup + disable ↻ UI).
    pub pulls_in_flight: Arc<Mutex<HashSet<String>>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionType {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elem: Option<Box<OptionType>>,
    /// Submodule field schemas (for attrsOf/listOf of submodule, or nested submodule types).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<OptionSchema>>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<i64>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<i64>,
    #[serde(default = "default_true")]
    pub iframeCompatible: bool,
}

#[derive(Serialize, Deserialize, Default)]
pub struct NavigatorContext {
    pub domain: Option<String>,
    pub services: Vec<ProxiedService>,
    #[serde(default)]
    pub theme: String,
    /// If set, the nix evaluator hit an error/timeout (e.g. flake eval taking >10min or broken flake.lock referencing missing store paths). The UI shows this message instead of hanging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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

/// Runtime unit for a service's status/logs/control pane section.
#[derive(Serialize, Clone)]
pub struct RuntimeUnit {
    pub name: String,
    /// True for docker-* units (or units backed by the containers registry); these get a manual docker update (pull) button.
    #[serde(default)]
    pub is_container: bool,
}

/// Context for the per-service option pane (includes both the form fields and rich intro metadata).
#[derive(Serialize)]
pub struct OptionPaneContext {
    pub service: String,
    pub meta: Option<ServiceMeta>,
    pub options: Vec<OptionSchema>,
    /// Pre-serialized JSON for the Alpine form (the options array only).
    pub options_json: String,
    /// Endpoint for save POST (e.g. /save/foo or /save-core/bar)
    #[serde(default)]
    pub save_endpoint: String,
    /// Whether this pane was loaded from the core grid (affects which back button is shown)
    #[serde(default)]
    pub is_core: bool,
    /// If set, the nix evaluator hit an error/timeout (e.g. flake eval taking >10min or broken flake.lock referencing missing store paths). The UI shows this message instead of hanging or empty pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Systemd units (without .service) declared for this neo service (for status/logs/control UI).
    #[serde(default)]
    pub units: Vec<RuntimeUnit>,
    /// Current container name -> image map for this service (from containers registry; editable in form).
    #[serde(default)]
    pub containers: std::collections::HashMap<String, String>,
}
