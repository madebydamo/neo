//! Domain types for the web UI (config, schema, services grid, versioning, option panes).
mod app_config;
mod eval_error;
mod pane;
mod schema;
mod services;
mod versioning;

pub use app_config::AppConfig;
pub use eval_error::{EvalErrorUi, EvalMark};
pub use pane::{OptionPaneContext, OptionSection, RuntimeUnit, ServiceMeta};
pub use schema::{ChoiceItem, OptionHelper, OptionSchema, OptionType, OptionUiOauth};
pub use services::{
    ConfigurationPageContext, ExtractedServiceGroups, IndexContext, NavigatorContext,
    PluginInventoryEntry, Service, ServicePlugin,
};
pub use versioning::ServicesAtRev;
