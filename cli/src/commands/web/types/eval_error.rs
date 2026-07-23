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

impl EvalErrorUi {
    pub fn from_failure(
        message: String,
        kind_id: String,
        can_store_repair: bool,
        can_flake_update: bool,
    ) -> Self {
        Self {
            error: Some(message),
            error_kind: Some(kind_id),
            can_store_repair,
            can_flake_update,
        }
    }

    pub fn message(msg: impl Into<String>) -> Self {
        Self {
            error: Some(msg.into()),
            ..Default::default()
        }
    }
}
