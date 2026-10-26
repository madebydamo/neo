//! Git plumbing and versioning helpers for the web UI.
mod changes;
mod dirty;
mod plumbing;
pub(crate) use plumbing::git_stdout;
mod versioning;

pub use changes::{collect_changes, lock_updates, settings_semantic, DiffRange};
pub use dirty::{dirty_state, is_worktree_dirty};
pub use versioning::{
    activation_branch_for_rev, activation_tips, enabled_services_at_rev, resolve_rev,
};
