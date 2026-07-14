use std::path::PathBuf;

use toml_edit::DocumentMut;

use super::save::after_save;
use crate::commands::paste_settings::paste_settings;
use crate::commands::web::structs::AppConfig;
use crate::commands::web::util::config_dir;

/// Restore working-tree `settings.toml` from last applied `/etc/neo/settings.toml`.
/// On success: same as after_save (refresh evaluator, action bar; schema invalidate if wired).
pub fn restore_settings_from_applied(config: &AppConfig) -> Result<(), String> {
    let dir = config_dir(config);
    let dir_str = dir.to_str().unwrap_or(".");
    let source = PathBuf::from("/etc/neo/settings.toml");
    let dummy = DocumentMut::new();
    paste_settings(dir_str, &source, &dummy, false, &config.nix_cmd)
        .map_err(|e| e.to_string())?;
    after_save(config);
    Ok(())
}
