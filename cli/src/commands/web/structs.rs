use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as AsyncMutex;

fn default_true() -> bool {
    true
}

fn default_generate_label() -> String {
    "Generate".to_string()
}

fn default_apply_set() -> String {
    "set".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct HelperInput {
    pub name: String,
    #[serde(rename = "type")]
    pub input_type: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionHelper {
    #[serde(default)]
    pub id: String,
    pub kind: String,
    #[serde(default = "default_generate_label")]
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_apply_set")]
    pub apply: String,
    /// Absolute script path from extract. Trusted only after server-side schema resolve.
    #[serde(default)]
    pub script: String,
    #[serde(default)]
    pub inputs: Vec<HelperInput>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Service {
    pub name: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<i64>,
    /// Category from service meta (e.g. Core, Network, Media).
    #[serde(default)]
    pub category: String,
    /// Long-form intro from service meta (info popover on the grid card).
    #[serde(default)]
    pub description: String,
}

/// One category bucket in the services grid.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ServiceCategoryGroup {
    pub name: String,
    pub services: Vec<Service>,
    /// At least one installed service in this group (for status filter UI).
    #[serde(default, rename = "hasEnabled")]
    pub has_enabled: bool,
    /// At least one uninstalled service in this group.
    #[serde(default, rename = "hasDisabled")]
    pub has_disabled: bool,
}

/// Shape returned by `extract_services.nix` before theme/error are filled in.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ExtractedServiceGroups {
    #[serde(default)]
    pub groups: Vec<ServiceCategoryGroup>,
    #[serde(default)]
    pub categories: Vec<String>,
}

#[derive(Serialize, Default)]
pub struct IndexContext {
    /// All services grouped by category (preferred category order).
    #[serde(default)]
    pub groups: Vec<ServiceCategoryGroup>,
    /// Category names for filter chips (same order as groups).
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub theme: String,
    /// If set, the nix evaluator hit an error/timeout (e.g. flake eval taking >10min or broken flake.lock referencing missing store paths). The UI shows this message instead of hanging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Full configuration shell page (not used by services_grid partial).
#[derive(Serialize)]
pub struct ConfigurationPageContext {
    pub theme: String,
    /// If set, the nix evaluator hit an error/timeout. Shown in the shell banner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Partial URL loaded into `#config-content` on first paint (HTMX GET).
    pub initial_content_url: String,
    /// Active tab seed: services | settings | versioning.
    pub initial_tab: String,
    /// Optional breadcrumb detail (service/section name) before first swap settles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_detail: Option<String>,
}

impl Default for ConfigurationPageContext {
    fn default() -> Self {
        Self {
            theme: String::new(),
            error: None,
            initial_content_url: "/configuration/services".to_string(),
            initial_tab: "services".to_string(),
            initial_detail: None,
        }
    }
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
    /// Process-local option schema cache for helper resolution (avoids re-taking eval mutex).
    pub schema_cache: Arc<tokio::sync::RwLock<super::schema_cache::SchemaCache>>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helper: Option<OptionHelper>,
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
    #[serde(default)]
    pub hostname: String,
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
