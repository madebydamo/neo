//! Web utility primitives: escaping, validation, paths, alerts, HTMX, in-flight sets.
mod alerts;
mod escape;
mod htmx;
mod inflight;
mod oob_status;
mod paths;
mod validate;

pub use alerts::{alert_html, changes_actions_row, AlertKind};
pub use escape::{escape_attr, escape_html, escape_nix_string};
pub use htmx::Htmx;
pub use inflight::InFlightSet;
pub use oob_status::{status_err, status_ok, status_pulling, status_slot_oob};
pub use paths::{config_dir, neo_bin, nix_bin, sudo_cmd};
pub use validate::{
    activation_id_ok, branch_ok, core_section_ok, generation_ok, repair_id_ok, rev_ok,
    service_name_ok, unit_name_valid,
};
