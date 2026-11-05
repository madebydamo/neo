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
    /// Long-form intro from service meta (grid search text).
    #[serde(default)]
    pub description: String,
    /// One-line summary on the grid card (meta tagline, else the first line of
    /// the description).
    #[serde(default)]
    pub tagline: String,
    /// Familiar products this service replaces (grid search text).
    #[serde(default)]
    pub replaces: Vec<String>,
    /// Plugin instance ids that declare this service (empty = core).
    #[serde(default, rename = "pluginIds")]
    pub plugin_ids: Vec<String>,
    /// Display badges (filled in Rust after extract; labels may overlap).
    #[serde(default)]
    pub plugins: Vec<ServicePlugin>,
    /// Systemd units of an installed service (grid status dot; empty when disabled).
    #[serde(default)]
    pub units: Vec<String>,
    /// Timer-backed subset of `units` (idle between runs; only failures count).
    #[serde(default)]
    pub timers: Vec<String>,
}

/// One plugin instance (service badge / filter dropdown entry). `id` is the identity.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ServicePlugin {
    pub id: String,
    pub url: String,
    pub display: String,
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

/// One applied plugin instance (input `plugin-<id>`) plus the services it declares.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PluginInventoryEntry {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub services: Vec<String>,
}

/// Shape returned by `extract_services.nix` before theme/error are filled in.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ExtractedServiceGroups {
    #[serde(default)]
    pub groups: Vec<ServiceCategoryGroup>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default, rename = "pluginInventory")]
    pub plugin_inventory: Vec<PluginInventoryEntry>,
    /// Guides tab and privacy map shown (neo.services.neo.guides).
    #[serde(default = "yes", rename = "guidesEnabled")]
    pub guides_enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Default)]
pub struct IndexContext {
    /// All services grouped by category (preferred category order).
    #[serde(default)]
    pub groups: Vec<ServiceCategoryGroup>,
    /// Category names for filter chips (same order as groups).
    #[serde(default)]
    pub categories: Vec<String>,
    /// Plugin filter dropdown (only when at least one plugin service exists).
    #[serde(default)]
    pub plugin_filters: Vec<ServicePlugin>,
    #[serde(default)]
    pub theme: String,
    /// Guides tab and privacy map shown.
    #[serde(default)]
    pub guides: bool,
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
    /// Unused when `initial_content_html` is already set (deep-linked option/core pane).
    pub initial_content_url: String,
    /// Active tab seed: services | settings | versioning.
    pub initial_tab: String,
    /// Optional breadcrumb detail (service/section name) before first swap settles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_detail: Option<String>,
    /// Server-rendered option/core pane HTML for full-page deep links so labels and
    /// fields are in the first response (no HTMX round-trip / works without JS).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_content_html: Option<String>,
    /// Assistant chat available (Hermes on with its chat key): drawer + buttons.
    #[serde(default)]
    pub assistant: bool,
    /// Guides tab shown (neo.services.neo.guides).
    #[serde(default)]
    pub guides: bool,
}

impl Default for ConfigurationPageContext {
    fn default() -> Self {
        Self {
            theme: String::new(),
            eval_error: EvalErrorUi::default(),
            initial_content_url: "/configuration".to_string(),
            initial_tab: "services".to_string(),
            initial_detail: None,
            initial_content_html: None,
            assistant: false,
            guides: true,
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
    /// Systemd units (sidebar status dot).
    #[serde(default)]
    pub units: Vec<String>,
    /// Timer-backed subset of `units`.
    #[serde(default)]
    pub timers: Vec<String>,
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
