// Ordered match rules for Nix error classification.
use super::super::NixErrorKind;

pub(super) struct Rule {
    /// Lower number = higher priority.
    pub priority: u8,
    pub kind: NixErrorKind,
    /// All needles must appear (case-insensitive substring) for a match.
    pub needles: &'static [&'static str],
    /// Optional extra predicate on the full text.
    #[allow(clippy::type_complexity)]
    pub extra: Option<fn(&str) -> bool>,
    pub summary: fn(&str, &[String]) -> String,
}

pub(super) fn summary_missing_path(_text: &str, paths: &[String]) -> String {
    match paths.first() {
        Some(p) => {
            format!("Store path is missing (often after GC or a lock from another machine): {p}")
        }
        None => "A referenced Nix store path does not exist".to_string(),
    }
}

pub(super) fn summary_hash(_text: &str, paths: &[String]) -> String {
    match paths.first() {
        Some(p) => format!("Fixed-output hash mismatch for {p}"),
        None => "Fixed-output derivation hash mismatch".to_string(),
    }
}

pub(super) fn summary_network(_text: &str, _paths: &[String]) -> String {
    "Failed to download a flake input or fixed-output source (network or remote error)".to_string()
}

pub(super) fn summary_sqlite(_text: &str, _paths: &[String]) -> String {
    "Nix database is busy or locked (another nix process may be running)".to_string()
}

pub(super) fn summary_flake_attr(_text: &str, _paths: &[String]) -> String {
    "Flake does not provide the expected attribute (check flake outputs / inputs)".to_string()
}

pub(super) fn summary_import(_text: &str, _paths: &[String]) -> String {
    "Nix could not import a file or module referenced by the configuration".to_string()
}

pub(super) fn summary_coerce(_text: &str, _paths: &[String]) -> String {
    "Type error during evaluation (cannot coerce or unexpected value type)".to_string()
}

pub(super) fn summary_ssl(_text: &str, _paths: &[String]) -> String {
    "TLS/SSL failure while fetching a remote flake input".to_string()
}

pub(super) fn summary_infinite(_text: &str, _paths: &[String]) -> String {
    "Nix hit infinite recursion while evaluating the configuration".to_string()
}

pub(super) fn summary_undefined(_text: &str, _paths: &[String]) -> String {
    "Undefined variable in the Nix configuration".to_string()
}

pub(super) fn summary_permission(_text: &str, _paths: &[String]) -> String {
    "Permission denied while accessing the Nix store or files".to_string()
}

pub(super) fn summary_assert(_text: &str, _paths: &[String]) -> String {
    "A Nix assertion or throw failed during evaluation".to_string()
}

pub(super) fn summary_timeout(_text: &str, _paths: &[String]) -> String {
    "Nix evaluation timed out waiting for a result marker".to_string()
}

pub(super) fn summary_died(_text: &str, _paths: &[String]) -> String {
    "The nix repl process exited unexpectedly".to_string()
}

pub(super) fn summary_lock(_text: &str, paths: &[String]) -> String {
    match paths.first() {
        Some(p) => format!(
            "flake.lock appears to reference a vanished path ({p}); update inputs or re-lock"
        ),
        None => "flake.lock references a path that is not available on this machine".to_string(),
    }
}

pub(super) fn summary_unknown(text: &str, _paths: &[String]) -> String {
    // Prefer the last `error:` line as summary.
    let leaf = text
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with("error:"))
        .map(|l| l.trim())
        .unwrap_or_else(|| {
            let t = text.trim();
            if t.len() > 200 {
                // first 200 chars
                let end = t.char_indices().nth(200).map(|(i, _)| i).unwrap_or(t.len());
                &t[..end]
            } else {
                t
            }
        });
    if leaf.is_empty() {
        "Unknown Nix evaluation error".to_string()
    } else {
        leaf.to_string()
    }
}

pub(super) const RULES: &[Rule] = &[
    Rule {
        priority: 10,
        kind: NixErrorKind::MissingStorePath,
        needles: &["does not exist"],
        extra: Some(|t| t.contains("/nix/store/")),
        summary: summary_missing_path,
    },
    Rule {
        priority: 10,
        kind: NixErrorKind::MissingStorePath,
        needles: &["no such file or directory"],
        extra: Some(|t| t.contains("/nix/store/")),
        summary: summary_missing_path,
    },
    Rule {
        priority: 10,
        kind: NixErrorKind::MissingStorePath,
        needles: &["is not valid"],
        extra: Some(|t| t.contains("/nix/store/")),
        summary: summary_missing_path,
    },
    Rule {
        priority: 15,
        kind: NixErrorKind::HashMismatch,
        needles: &["hash mismatch"],
        extra: None,
        summary: summary_hash,
    },
    Rule {
        priority: 15,
        kind: NixErrorKind::HashMismatch,
        needles: &["narhashmismatch"],
        extra: None,
        summary: summary_hash,
    },
    Rule {
        priority: 15,
        kind: NixErrorKind::HashMismatch,
        needles: &["specified:", "got:"],
        extra: Some(|t| {
            let l = t.to_lowercase();
            l.contains("hash") || t.contains("sha256") || l.contains("nar")
        }),
        summary: summary_hash,
    },
    Rule {
        priority: 15,
        kind: NixErrorKind::HashMismatch,
        needles: &["fixed-output derivation produced path"],
        extra: Some(|t| t.to_lowercase().contains("hash")),
        summary: summary_hash,
    },
    Rule {
        priority: 18,
        kind: NixErrorKind::NetworkFetchFailed,
        needles: &["unable to download"],
        extra: None,
        summary: summary_network,
    },
    Rule {
        priority: 18,
        kind: NixErrorKind::NetworkFetchFailed,
        needles: &["http error"],
        extra: None,
        summary: summary_network,
    },
    Rule {
        priority: 18,
        kind: NixErrorKind::NetworkFetchFailed,
        needles: &["connection refused"],
        extra: None,
        summary: summary_network,
    },
    Rule {
        priority: 18,
        kind: NixErrorKind::NetworkFetchFailed,
        needles: &["could not download"],
        extra: None,
        summary: summary_network,
    },
    Rule {
        priority: 18,
        kind: NixErrorKind::NetworkFetchFailed,
        needles: &["failed to fetch"],
        extra: None,
        summary: summary_network,
    },
    Rule {
        priority: 18,
        kind: NixErrorKind::NetworkFetchFailed,
        needles: &["network is unreachable"],
        extra: None,
        summary: summary_network,
    },
    Rule {
        priority: 18,
        kind: NixErrorKind::NetworkFetchFailed,
        needles: &["temporary failure in name resolution"],
        extra: None,
        summary: summary_network,
    },
    Rule {
        priority: 18,
        kind: NixErrorKind::NetworkFetchFailed,
        needles: &["could not resolve host"],
        extra: None,
        summary: summary_network,
    },
    Rule {
        priority: 18,
        kind: NixErrorKind::NetworkFetchFailed,
        needles: &["ssl"],
        extra: Some(|t| {
            let l = t.to_lowercase();
            l.contains("certificate") || l.contains("handshake") || l.contains("tls")
        }),
        summary: summary_ssl,
    },
    Rule {
        priority: 18,
        kind: NixErrorKind::NetworkFetchFailed,
        needles: &["tls"],
        extra: Some(|t| {
            let l = t.to_lowercase();
            l.contains("error") || l.contains("handshake") || l.contains("certificate")
        }),
        summary: summary_ssl,
    },
    Rule {
        priority: 22,
        kind: NixErrorKind::PermissionDenied,
        needles: &["sqlite database is busy"],
        extra: None,
        summary: summary_sqlite,
    },
    Rule {
        priority: 22,
        kind: NixErrorKind::PermissionDenied,
        needles: &["database is locked"],
        extra: Some(|t| t.to_lowercase().contains("sqlite") || t.to_lowercase().contains("nix")),
        summary: summary_sqlite,
    },
    Rule {
        priority: 25,
        kind: NixErrorKind::InfiniteRecursion,
        needles: &["infinite recursion"],
        extra: None,
        summary: summary_infinite,
    },
    Rule {
        priority: 25,
        kind: NixErrorKind::InfiniteRecursion,
        needles: &["circular import"],
        extra: None,
        summary: summary_infinite,
    },
    Rule {
        priority: 25,
        kind: NixErrorKind::UndefinedVariable,
        needles: &["undefined variable"],
        extra: None,
        summary: summary_undefined,
    },
    Rule {
        priority: 25,
        kind: NixErrorKind::PermissionDenied,
        needles: &["permission denied"],
        extra: None,
        summary: summary_permission,
    },
    Rule {
        priority: 25,
        kind: NixErrorKind::PermissionDenied,
        needles: &["operation not permitted"],
        extra: None,
        summary: summary_permission,
    },
    Rule {
        priority: 28,
        kind: NixErrorKind::EvalAssertion,
        needles: &["does not provide attribute"],
        extra: None,
        summary: summary_flake_attr,
    },
    Rule {
        priority: 28,
        kind: NixErrorKind::EvalAssertion,
        needles: &["flake '", "does not provide"],
        extra: None,
        summary: summary_flake_attr,
    },
    Rule {
        priority: 28,
        kind: NixErrorKind::EvalAssertion,
        needles: &["cannot import"],
        extra: None,
        summary: summary_import,
    },
    Rule {
        priority: 28,
        kind: NixErrorKind::EvalAssertion,
        needles: &["cannot coerce"],
        extra: None,
        summary: summary_coerce,
    },
    Rule {
        priority: 28,
        kind: NixErrorKind::EvalAssertion,
        needles: &["value is a function while a set was expected"],
        extra: None,
        summary: summary_coerce,
    },
    Rule {
        priority: 28,
        kind: NixErrorKind::EvalAssertion,
        needles: &["value is a string while a set was expected"],
        extra: None,
        summary: summary_coerce,
    },
    Rule {
        priority: 30,
        kind: NixErrorKind::EvalAssertion,
        needles: &["assertion"],
        extra: Some(|t| {
            let l = t.to_lowercase();
            l.contains("failed") || l.contains("assertion")
        }),
        summary: summary_assert,
    },
    Rule {
        priority: 30,
        kind: NixErrorKind::EvalAssertion,
        needles: &["error: throw"],
        extra: None,
        summary: summary_assert,
    },
    Rule {
        priority: 30,
        kind: NixErrorKind::Timeout,
        needles: &["timeout waiting for marker"],
        extra: None,
        summary: summary_timeout,
    },
    Rule {
        priority: 30,
        kind: NixErrorKind::Timeout,
        needles: &["no marker from repl"],
        extra: None,
        summary: summary_timeout,
    },
    Rule {
        priority: 30,
        kind: NixErrorKind::ProcessDied,
        needles: &["stdout closed"],
        extra: None,
        summary: summary_died,
    },
    // path:/ git+file: locks from another machine (often without flake.lock in the message).
    Rule {
        priority: 35,
        kind: NixErrorKind::FlakeLockStale,
        needles: &["git+file:"],
        extra: Some(|t| {
            let l = t.to_lowercase();
            l.contains("does not exist")
                || l.contains("no such file")
                || l.contains("error:")
                || l.contains("failed")
        }),
        summary: summary_lock,
    },
    Rule {
        priority: 35,
        kind: NixErrorKind::FlakeLockStale,
        needles: &["path:"],
        extra: Some(|t| {
            let l = t.to_lowercase();
            (l.contains("does not exist") || l.contains("no such file"))
                && (l.contains("flake") || l.contains("input") || l.contains("lock"))
        }),
        summary: summary_lock,
    },
    // flake.lock + missing path is already MissingStorePath; this catches lock wording alone.
    Rule {
        priority: 40,
        kind: NixErrorKind::FlakeLockStale,
        needles: &["flake.lock"],
        extra: Some(|t| {
            let l = t.to_lowercase();
            l.contains("does not exist")
                || l.contains("no such file")
                || l.contains("locked")
                || l.contains("outdated")
        }),
        summary: summary_lock,
    },
    Rule {
        priority: 40,
        kind: NixErrorKind::FlakeLockStale,
        needles: &["locked input"],
        extra: Some(|t| {
            let l = t.to_lowercase();
            l.contains("does not exist") || l.contains("error") || l.contains("failed")
        }),
        summary: summary_lock,
    },
];
