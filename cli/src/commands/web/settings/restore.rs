use std::path::Path;

use toml_edit::DocumentMut;

use super::save::refresh_after_settings_change;
use crate::commands::paste_settings::paste_settings;
use crate::commands::web::git::git_stdout;
use crate::commands::web::types::AppConfig;

/// Discard pending changes: restore every tracked file (index + worktree) to `HEAD`,
/// i.e. the last committed activation/build. Untracked files are left alone.
/// Falls back to `/etc/neo/settings.toml` only when the repo has no commit yet.
/// On success: refresh evaluator, action bar, and schema cache.
pub fn discard_pending_changes(config: &AppConfig) -> Result<(), String> {
    let dir = config.config_dir();
    if git_stdout(&dir, &["rev-parse", "--verify", "HEAD"]).is_ok() {
        git_stdout(
            &dir,
            &[
                "restore",
                "--source=HEAD",
                "--staged",
                "--worktree",
                "--",
                ".",
            ],
        )?;
    } else {
        let dir_str = dir.to_str().unwrap_or(".");
        let source = Path::new("/etc/neo/settings.toml");
        let dummy = DocumentMut::new();
        paste_settings(dir_str, source, &dummy, false).map_err(|e| e.to_string())?;
    }
    refresh_after_settings_change(config);
    Ok(())
}
