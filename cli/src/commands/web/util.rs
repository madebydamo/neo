use std::path::PathBuf;

use super::structs::AppConfig;

pub fn config_dir(cfg: &AppConfig) -> PathBuf {
    cfg.settings_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn sudo_cmd() -> String {
    std::env::var("SUDO_BINARY_PATH").unwrap_or_else(|_| "sudo".to_string())
}
