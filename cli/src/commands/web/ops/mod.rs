//! Background operation store and related UI fragments.
pub mod genswitch;
pub mod store;
pub mod timeline;

pub use store::{
    append_log, find_recent_in_progress, gc_old_ops, load_log_tail, load_state, log_path, ops_dir,
    state_fields, state_path, write_state,
};
