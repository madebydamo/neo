//! Shared helpers used by CLI commands and the web UI.
//!
//! Keep `crate::commands` for subcommand entry points only.

pub mod command;
pub mod generation;
pub mod locks;
pub mod ops;
pub mod profile;
pub mod secrets;
pub mod secrets_store;
pub mod settings;
pub mod stdio_tee;
pub mod template;
pub mod toml_sort;

pub use command::{
    execute_command, format_command, get_current_branch, get_timestamp, git_cmd,
    has_staged_changes, run_nix, run_write_flake,
};
pub use generation::{
    current_generation_number, parse_generation_from_message, record_generation_in_commit,
    running_generation_number, switch_system_generation, system_profile_available, GenerationMode,
    GenerationTimeline,
};
pub use ops::{resolve_suffix, OperationKind, OperationLog, OPERATIONS_DIR};
pub use profile::{
    is_local_flake_ref, neo_cli_get, resolve_config_path, resolve_profile, resolve_template,
    set_profile_str,
};
pub use settings::{disko_enabled, load_or_default_settings};
pub use toml_sort::sort_document_alphabetically;
