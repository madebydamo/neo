//! Shared helpers used by CLI commands and the web UI.
//!
//! Keep `crate::commands` for subcommand entry points only.

pub mod backup;
pub mod bins;
pub mod command;
pub mod docker_ledger;
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
