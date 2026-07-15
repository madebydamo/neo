// Map NixErrorKind → remediation plans (UI actions / operator steps).
use super::NixErrorKind;

/// A single operator-facing action the UI may offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemediationAction {
    /// `sudo -n nix-store --verify --repair`
    StoreVerifyRepair,
    /// Trigger flake/input update (existing web action).
    FlakeUpdate,
}

impl RemediationAction {
    #[allow(dead_code)] // reserved for structured action lists in the UI
    pub fn id(self) -> &'static str {
        match self {
            Self::StoreVerifyRepair => "store-verify-repair",
            Self::FlakeUpdate => "flake-update",
        }
    }

    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Self::StoreVerifyRepair => "Repair Nix store",
            Self::FlakeUpdate => "Update flake inputs",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemediationPlan {
    pub actions: Vec<RemediationAction>,
    pub help: &'static str,
}

pub fn plan_for(kind: NixErrorKind) -> RemediationPlan {
    match kind {
        NixErrorKind::MissingStorePath => RemediationPlan {
            actions: vec![
                RemediationAction::StoreVerifyRepair,
                RemediationAction::FlakeUpdate,
            ],
            help: "Missing store paths are often fixed by repairing the store. If the path never existed on this machine (lock from a laptop path:/…), update flake inputs instead.",
        },
        NixErrorKind::HashMismatch => RemediationPlan {
            actions: vec![RemediationAction::StoreVerifyRepair],
            help: "A fixed-output derivation hash does not match. Store repair may re-fetch the path.",
        },
        NixErrorKind::FlakeLockStale => RemediationPlan {
            actions: vec![RemediationAction::FlakeUpdate],
            help: "flake.lock points at something unavailable here. Refresh inputs or re-lock on this host.",
        },
        NixErrorKind::NetworkFetchFailed => RemediationPlan {
            actions: vec![RemediationAction::FlakeUpdate],
            help: "neo-web already retries a network fetch once. If it still fails, check network / DNS / proxy, or update flake inputs to pin reachable revisions.",
        },
        NixErrorKind::Timeout | NixErrorKind::ProcessDied => RemediationPlan {
            actions: vec![],
            help: "The evaluator was restarted. Reload the page; if it keeps failing, check neo-web logs.",
        },
        NixErrorKind::InfiniteRecursion
        | NixErrorKind::UndefinedVariable
        | NixErrorKind::EvalAssertion => RemediationPlan {
            actions: vec![],
            help: "This is a configuration evaluation bug — fix the Nix modules / settings, then reload.",
        },
        NixErrorKind::PermissionDenied => RemediationPlan {
            actions: vec![],
            help: "Check Nix store permissions and that neo-web can run nix as expected.",
        },
        NixErrorKind::Unknown => RemediationPlan {
            actions: vec![],
            help: "See neo-web journal logs for the full Nix trace.",
        },
    }
}

pub fn offers_store_repair(kind: NixErrorKind) -> bool {
    plan_for(kind)
        .actions
        .contains(&RemediationAction::StoreVerifyRepair)
}

pub fn offers_flake_update(kind: NixErrorKind) -> bool {
    plan_for(kind)
        .actions
        .contains(&RemediationAction::FlakeUpdate)
}
