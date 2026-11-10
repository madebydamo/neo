use serde::{Deserialize, Serialize};

/// Shared Nix eval failure fields for UI banners (services grid, config shell, panes, nav).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EvalErrorUi {
    /// If set, the nix evaluator hit an error/timeout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Stable kind id from the classifier (e.g. missing-store-path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
    /// Offer the "Repair Nix store" button for remediable store failures.
    #[serde(default)]
    pub can_store_repair: bool,
    /// Offer "Update flake inputs" when the lock/inputs look stale or machine-local.
    #[serde(default)]
    pub can_flake_update: bool,
}

/// Which evaluation a page was rendered from (docs/design/eval-latency.md §7).
/// `provisional`: settings changed since, and were laid over it; the next
/// evaluation replaces the page if it differs.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvalMark {
    pub gen: u64,
    pub provisional: bool,
}

impl EvalErrorUi {
    pub fn message(msg: impl Into<String>) -> Self {
        Self {
            error: Some(msg.into()),
            ..Default::default()
        }
    }
}
