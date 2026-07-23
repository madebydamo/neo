//! Shared helpers used by CLI commands and the web UI.
//!
//! Keep `crate::commands` for subcommand entry points only.

pub mod command;
pub mod generation;
pub mod ops;
pub mod profile;
pub mod settings;
pub mod toml_sort;

pub use command::{
    execute_command, format_command, get_current_branch, get_timestamp, git_cmd,
    has_staged_changes, run_nix, shell_join, shell_quote,
};
pub use generation::{
    activation_commit_message, current_generation_number, list_system_generations,
    list_system_generations_with_sudo, parse_generation_from_message, record_generation_in_commit,
    switch_system_generation, GenerationMode, GenerationsList, SystemGeneration,
};
pub use ops::{operations_dir, resolve_suffix, OperationKind, OperationLog, OPERATIONS_DIR};
pub use profile::{
    neo_cli_get, normalize_profile_arg, resolve_config_path, resolve_profile, PROFILE_LOCAL,
    PROFILE_SERVER,
};
pub use settings::load_or_default_settings;
pub use toml_sort::sort_document_alphabetically;
