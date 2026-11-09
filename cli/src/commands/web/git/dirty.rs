//! Dirty worktree / settings.toml detection for the action bar.
use std::path::Path;

use super::super::types::AppConfig;
use super::plumbing::git_dirty;

/// Combined dirty flags for action-bar / changes UI (one pass of git checks).
pub struct DirtyState {
    pub settings_dirty: bool,
    pub worktree_dirty: bool,
}

/// Run settings + worktree dirty checks.
pub fn dirty_state(cfg: &AppConfig) -> DirtyState {
    let dir = cfg.config_dir();
    let settings_dirty = git_dirty(&dir, Some("settings.toml"));
    let worktree_dirty = git_dirty(&dir, None);
    DirtyState {
        settings_dirty,
        worktree_dirty,
    }
}

pub fn is_worktree_dirty(dir: &Path) -> bool {
    git_dirty(dir, None)
}
