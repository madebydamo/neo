use serde::{Deserialize, Serialize};

use super::eval_error::EvalErrorUi;
use super::schema::OptionSchema;

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
    #[serde(flatten)]
    pub eval_error: EvalErrorUi,
    /// Systemd units (without .service) declared for this neo service (for status/logs/control UI).
    #[serde(default)]
    pub units: Vec<RuntimeUnit>,
    /// Current container name -> image map for this service (from containers registry; editable in form).
    #[serde(default)]
    pub containers: std::collections::HashMap<String, String>,
    /// Host path of this service's appdata directory (from mkAppdata); enables Clear appdata in the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appdata: Option<String>,
    /// Global AppData volume root (`neo.core.volumes.appdata`); used to validate clear-appdata paths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appdata_root: Option<String>,
}
