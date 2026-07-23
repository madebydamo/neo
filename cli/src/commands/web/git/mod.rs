//! Git plumbing and versioning helpers for the web UI.
mod dirty;
mod plumbing;
mod versioning;

pub use dirty::{dirty_state, is_worktree_dirty, DirtyState};
pub use versioning::{
    activation_branch_for_rev, activation_graph, diff_settings, enabled_services_at_rev,
    get_settings_toml_diff, list_activation_branches, resolve_rev, settings_at_rev,
};
