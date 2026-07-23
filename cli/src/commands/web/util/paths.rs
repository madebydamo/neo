use std::path::{Path, PathBuf};

/// Parent directory of settings.toml (config repo root).
pub fn config_dir(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn sudo_cmd() -> String {
    std::env::var("SUDO_BINARY_PATH").unwrap_or_else(|_| "sudo".to_string())
}

pub fn nix_bin() -> String {
    std::env::var("NIX_BINARY_PATH")
        .unwrap_or_else(|_| "/run/current-system/sw/bin/nix".to_string())
}

pub fn neo_bin() -> String {
    std::env::var("NEO_BINARY_PATH")
        .unwrap_or_else(|_| "/run/current-system/sw/bin/neo".to_string())
}
