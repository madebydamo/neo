//! Domain types for the web UI (config, schema, services grid, versioning, option panes).
mod app_config;
mod eval_error;
mod pane;
mod schema;
mod services;
mod versioning;

pub use app_config::AppConfig;
pub use eval_error::EvalErrorUi;
pub use pane::{OptionPaneContext, RuntimeUnit, ServiceMeta, ServiceScreenshot};
pub use schema::{
    HelperInput, OptionHelper, OptionSchema, OptionType, OptionUi, OptionUiKeysFrom, OptionUiMode,
    OptionUiSave,
};
pub use services::{
    ConfigurationPageContext, ExtractedServiceGroups, IndexContext, NavigatorContext,
    ProxiedService, Service, ServiceCategoryGroup,
};
pub use versioning::{BranchInfo, BranchesContext, GraphCommit, ServicesAtRev, VersioningGraph};
