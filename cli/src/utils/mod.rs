//! Shared helpers used by CLI commands and the web UI.
//!
//! Keep `crate::commands` for subcommand entry points only.

pub mod backup;
pub mod bins;
pub mod command;
pub mod fsutil;
pub mod generation;
pub mod locks;
pub mod ops;
pub mod plugins;
pub mod profile;
pub mod secrets_store;
pub mod secrets_yaml;
pub mod settings;
pub mod stdio_tee;
pub mod template;
pub mod toml_sort;

// Kept only for the web UI (`commands/web/**`), which still imports these
// from here; everything else imports from the submodules.
pub use command::{get_timestamp, git_cmd};
pub use generation::{
    current_generation_number, parse_generation_from_message, running_generation_number,
    switch_system_generation, system_profile_available, GenerationMode, GenerationTimeline,
};
pub use ops::{OperationKind, OperationLog, OPERATIONS_DIR};
// The web UI still imports the YAML codec under its old name `utils::secrets`.
pub use secrets_yaml as secrets;
pub use toml_sort::sort_document_alphabetically;
