// Classify raw Nix evaluator / stderr text into stable error kinds for the UI.
mod patterns;
mod remediate;

pub use patterns::classify;
pub use remediate::{
    offers_flake_update, offers_store_repair, plan_for, RemediationAction, RemediationPlan,
};

/// Stable categories operators and remediation code can switch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NixErrorKind {
    MissingStorePath,
    FlakeLockStale,
    EvalAssertion,
    InfiniteRecursion,
    UndefinedVariable,
    HashMismatch,
    NetworkFetchFailed,
    PermissionDenied,
    Timeout,
    ProcessDied,
    Unknown,
}

impl NixErrorKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::MissingStorePath => "missing-store-path",
            Self::FlakeLockStale => "flake-lock-stale",
            Self::EvalAssertion => "eval-assertion",
            Self::InfiniteRecursion => "infinite-recursion",
            Self::UndefinedVariable => "undefined-variable",
            Self::HashMismatch => "hash-mismatch",
            Self::NetworkFetchFailed => "network-fetch-failed",
            Self::PermissionDenied => "permission-denied",
            Self::Timeout => "timeout",
            Self::ProcessDied => "process-died",
            Self::Unknown => "unknown",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::MissingStorePath => "Missing store path",
            Self::FlakeLockStale => "Stale flake lock",
            Self::EvalAssertion => "Assertion failed",
            Self::InfiniteRecursion => "Infinite recursion",
            Self::UndefinedVariable => "Undefined variable",
            Self::HashMismatch => "Hash mismatch",
            Self::NetworkFetchFailed => "Network fetch failed",
            Self::PermissionDenied => "Permission denied",
            Self::Timeout => "Evaluation timeout",
            Self::ProcessDied => "Nix process died",
            Self::Unknown => "Nix evaluation error",
        }
    }
}

/// Structured Nix failure for banners, logs, and future remediation.
#[derive(Debug, Clone)]
pub struct NixError {
    pub kind: NixErrorKind,
    /// One-line operator-facing summary.
    pub summary: String,
    /// Truncated multi-line detail (stderr / anyhow chain).
    pub detail: String,
    /// Store paths extracted from the message when present.
    pub paths: Vec<String>,
}

impl NixError {
    pub fn classify(text: &str) -> Self {
        classify(text)
    }

    /// Compact message for existing `error: Option<String>` template fields.
    pub fn user_message(&self) -> String {
        let mut msg = format!("[{}] {}", self.kind.id(), self.summary);
        if let Some(p) = self.paths.first() {
            if !self.summary.contains(p) {
                msg.push_str(&format!(" ({p})"));
            }
        }
        msg
    }

    /// Longer message for panes / banners (summary + short detail tail).
    pub fn display_message(&self) -> String {
        let detail = self.detail.trim();
        if detail.is_empty() || detail == self.summary {
            return self.user_message();
        }
        let tail = if detail.len() > 800 {
            // Prefer leaf error at the end of Nix traces.
            let start = detail
                .char_indices()
                .rev()
                .nth(799)
                .map(|(i, _)| i)
                .unwrap_or(0);
            format!("…{}", &detail[start..])
        } else {
            detail.to_string()
        };
        format!("{}\n{}", self.user_message(), tail)
    }
}

impl std::fmt::Display for NixError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.user_message())
    }
}
