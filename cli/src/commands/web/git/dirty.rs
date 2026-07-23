//! Dirty worktree / settings.toml detection for the action bar.
use std::path::Path;

use super::super::types::AppConfig;
use super::super::util::config_dir;
use super::plumbing::{git_dirty, worktree_summary};

/// Combined dirty flags for action-bar / changes UI (one pass of git checks).
pub struct DirtyState {
    pub settings_dirty: bool,
    pub worktree_dirty: bool,
    pub summary: String,
}

/// Run settings + worktree dirty checks efficiently (summary only when worktree dirty).
pub fn dirty_state(cfg: &AppConfig) -> DirtyState {
    let dir = config_dir(&cfg.settings_path);
    let settings_dirty = git_dirty(&dir, Some("settings.toml"));
    let worktree_dirty = git_dirty(&dir, None);
    let summary = if worktree_dirty {
        worktree_summary(&dir)
    } else {
        String::new()
    };
    DirtyState {
        settings_dirty,
        worktree_dirty,
        summary,
    }
}

pub fn is_worktree_dirty(config_path: &str) -> bool {
    git_dirty(Path::new(config_path), None)
}
