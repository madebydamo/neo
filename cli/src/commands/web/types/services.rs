use serde::{Deserialize, Serialize};

use super::eval_error::EvalErrorUi;

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
    #[serde(flatten)]
    pub eval_error: EvalErrorUi,
}

/// Full configuration shell page (not used by services_grid partial).
#[derive(Serialize)]
pub struct ConfigurationPageContext {
    pub theme: String,
    #[serde(flatten)]
    pub eval_error: EvalErrorUi,
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
            eval_error: EvalErrorUi::default(),
            initial_content_url: "/configuration".to_string(),
            initial_tab: "services".to_string(),
            initial_detail: None,
        }
    }
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
    #[serde(flatten)]
    pub eval_error: EvalErrorUi,
}
