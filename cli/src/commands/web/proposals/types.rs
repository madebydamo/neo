//! What proposals are made of: the changes the assistant sends, what a card
//! shows, and the plan Save carries out.
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::web::settings::edit::PatchOp;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChangeOp {
    Set,
    Unset,
    Add,
    Remove,
    Generate,
    Ask,
    Copy,
    Move,
}

/// The setting a `copy` / `move` reads: an option, one entry, or a field.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct SourceRef {
    pub path: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub field: Option<String>,
}

impl SourceRef {
    pub(super) fn dotted(&self) -> String {
        std::iter::once(self.path.as_str())
            .chain(self.key.as_deref())
            .chain(self.field.as_deref())
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// One change as the assistant sends it.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ChangeRequest {
    pub op: ChangeOp,
    /// Option path, e.g. `services.pihole.localIP` or `core.timeZone`.
    pub path: String,
    /// Entry name for attrsOf options (`accounts` → `tmr`).
    #[serde(default)]
    pub key: Option<String>,
    /// Field inside the entry (or submodule), dotted: `password`, `oauth.clientId`.
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub value: Option<Value>,
    /// `ask` changes with the same group share one input on the card.
    #[serde(default)]
    pub group: Option<String>,
    /// What `copy` / `move` read.
    #[serde(default)]
    pub from: Option<SourceRef>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ProposalRequest {
    pub summary: String,
    pub changes: Vec<ChangeRequest>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Saved,
    Applied,
    Rejected,
    Stale,
    Failed,
    /// A Save click is running (claimed by [`claim`]); not actionable, so a
    /// second click cannot save it twice.
    Saving,
}

impl Status {
    /// Waiting for the user (a failed save can be tried again).
    pub fn is_actionable(&self) -> bool {
        matches!(self, Status::Pending | Status::Failed)
    }
}

/// What the card shows for one change.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChangeView {
    pub service: String,
    /// Where the change lands, e.g. `Accounts › tmr › Password`.
    pub label: String,
    /// Option path plus entry and field (`services.x.accounts.tmr.password`).
    pub path: String,
    /// `add` (new entry or list item), `change`, `set` (was not set),
    /// `remove` (entry or list item), `reset` (back to the default).
    pub action: String,
    pub old: String,
    pub new: String,
    pub secret: bool,
    /// Entries a whole-map write removes ("Removes entry tinyauth").
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Entries it adds or changes.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// A password input on the card (op `ask`), shared by changes of one group.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AskInput {
    /// Form field name (`ask_<group>`).
    pub name: String,
    pub label: String,
    /// The settings it fills, when more than one.
    pub used_for: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub(super) enum Planned {
    Patch(PatchOp),
    /// Secret filled by the field's own helper script when the user saves.
    Generate {
        path: Vec<String>,
        script: String,
    },
    /// Secret the user enters on the card; the value comes with the save.
    Ask {
        path: Vec<String>,
        input: String,
    },
    /// Value of another setting, secrets included, read when the user saves.
    Copy {
        from: SourceRef,
        from_path: Vec<String>,
        to: Vec<String>,
        /// A move: the source goes away.
        remove_source: bool,
        /// The source has secrets inside.
        secret: bool,
    },
}

impl Planned {
    /// Settings this change depends on.
    pub(super) fn paths(&self) -> Vec<&[String]> {
        match self {
            Planned::Patch(PatchOp::Set { path, .. })
            | Planned::Patch(PatchOp::Unset { path, .. })
            | Planned::Generate { path, .. }
            | Planned::Ask { path, .. } => vec![path],
            Planned::Copy { from_path, to, .. } => vec![to, from_path],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Proposal {
    pub id: String,
    pub summary: String,
    pub views: Vec<ChangeView>,
    #[serde(default)]
    pub asks: Vec<AskInput>,
    pub(super) planned: Vec<Planned>,
    pub(super) fingerprint: String,
    pub status: Status,
    pub message: String,
    /// Unix seconds.
    pub(super) created: u64,
}
