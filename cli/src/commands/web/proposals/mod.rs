//! Settings proposals: changes the assistant suggests by option path, checked
//! against the option schema and saved only when the user clicks in the web
//! UI, through the same [`apply_edit`] as the settings form
//! (docs/design/descriptions-and-guides.md §3b).
//!
//! A change addresses an option (`path`), one named entry of it (`key`) and a
//! field inside that (`field`, dotted). Secret values never travel through the
//! assistant: `generate` runs the field's Generate script on save, `ask` puts a
//! password input on the card whose value comes with the Save click and goes
//! straight into the patch, and `copy` / `move` name another setting (`from`)
//! whose saved value is read on save, so the assistant can reorganise settings
//! with secrets inside without seeing them. Proposals hold no secret values and
//! are kept in a JSON file in neo-web's state directory, so a card survives the
//! restart that Save & apply causes.

mod check;
mod plan;
mod save;
mod store;
#[cfg(test)]
mod tests;
mod types;

pub use plan::create;
pub use save::ops_for_save;
pub use store::{claim, get, set_status};
pub use types::{AskInput, ChangeView, Proposal, ProposalRequest, Status};
