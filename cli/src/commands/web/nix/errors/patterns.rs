// Ordered match rules: first hit wins (priority is list order within priority tiers).
use super::{NixError, NixErrorKind};

struct Rule {
    /// Lower number = higher priority.
    priority: u8,
    kind: NixErrorKind,
    /// All needles must appear (case-insensitive substring) for a match.
    needles: &'static [&'static str],
    /// Optional extra predicate on the full text.
    #[allow(clippy::type_complexity)]
    extra: Option<fn(&str) -> bool>,
    summary: fn(&str, &[String]) -> String,
}

fn extract_store_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let prefix = b"/nix/store/";
    let mut i = 0;
    while i + prefix.len() < bytes.len() {
        if bytes[i..].starts_with(prefix) {
            let start = i;
            i += prefix.len();
            while i < bytes.len() {
                let c = bytes[i];
                // Store path characters: base32 hash + name (alnum, -, ., _, +)
                if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.' | b'_' | b'+' | b'/') {
                    i += 1;
                } else {
                    break;
                }
            }
            // Trim trailing punctuation that is not part of the path.
            let mut end = i;
            while end > start {
                let c = bytes[end - 1];
                if matches!(c, b'\'' | b'"' | b',' | b')' | b']' | b';' | b':' | b'.') {
                    end -= 1;
                } else {
                    break;
                }
            }
            // Prefer the store object root (hash-name), drop trailing file segments for display
            // when path is like ...-source/flake.nix — keep full path for repair targeting.
            if end > start {
                let path = String::from_utf8_lossy(&bytes[start..end]).into_owned();
                if !out.iter().any(|p| p == &path) {
                    out.push(path);
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

fn lower_contains_all(text_lower: &str, needles: &[&str]) -> bool {
    needles.iter().all(|n| text_lower.contains(&n.to_lowercase()))
}

fn summary_missing_path(_text: &str, paths: &[String]) -> String {
    match paths.first() {
        Some(p) => format!(
            "Store path is missing (often after GC or a lock from another machine): {p}"
        ),
        None => "A referenced Nix store path does not exist".to_string(),
    }
}

fn summary_hash(_text: &str, paths: &[String]) -> String {
    match paths.first() {
        Some(p) => format!("Fixed-output hash mismatch for {p}"),
        None => "Fixed-output derivation hash mismatch".to_string(),
    }
}

fn summary_network(_text: &str, _paths: &[String]) -> String {
    "Failed to download a flake input or fixed-output source (network or remote error)".to_string()
}

fn summary_sqlite(_text: &str, _paths: &[String]) -> String {
    "Nix database is busy or locked (another nix process may be running)".to_string()
}

fn summary_flake_attr(_text: &str, _paths: &[String]) -> String {
    "Flake does not provide the expected attribute (check flake outputs / inputs)".to_string()
}

fn summary_import(_text: &str, _paths: &[String]) -> String {
    "Nix could not import a file or module referenced by the configuration".to_string()
}

fn summary_coerce(_text: &str, _paths: &[String]) -> String {
    "Type error during evaluation (cannot coerce or unexpected value type)".to_string()
}

fn summary_ssl(_text: &str, _paths: &[String]) -> String {
    "TLS/SSL failure while fetching a remote flake input".to_string()
}

fn summary_infinite(_text: &str, _paths: &[String]) -> String {
    "Nix hit infinite recursion while evaluating the configuration".to_string()
}

fn summary_undefined(_text: &str, _paths: &[String]) -> String {
    "Undefined variable in the Nix configuration".to_string()
}

fn summary_permission(_text: &str, _paths: &[String]) -> String {
    "Permission denied while accessing the Nix store or files".to_string()
}

fn summary_assert(_text: &str, _paths: &[String]) -> String {
    "A Nix assertion or throw failed during evaluation".to_string()
}

fn summary_timeout(_text: &str, _paths: &[String]) -> String {
    "Nix evaluation timed out waiting for a result marker".to_string()
}

fn summary_died(_text: &str, _paths: &[String]) -> String {
    "The nix repl process exited unexpectedly".to_string()
}

fn summary_lock(_text: &str, paths: &[String]) -> String {
    match paths.first() {
        Some(p) => format!(
            "flake.lock appears to reference a vanished path ({p}); update inputs or re-lock"
        ),
        None => "flake.lock references a path that is not available on this machine".to_string(),
    }
}

fn summary_unknown(text: &str, _paths: &[String]) -> String {
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
                let end = t
                    .char_indices()
                    .nth(200)
                    .map(|(i, _)| i)
                    .unwrap_or(t.len());
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

const RULES: &[Rule] = &[
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

fn trim_detail(text: &str) -> String {
    let t = text.trim();
    const MAX: usize = 4000;
    if t.len() <= MAX {
        return t.to_string();
    }
    let start = t
        .char_indices()
        .rev()
        .nth(MAX - 1)
        .map(|(i, _)| i)
        .unwrap_or(0);
    format!("…{}", &t[start..])
}

/// Classify raw error / stderr text. Pure function — safe to unit-test with fixtures.
pub fn classify(text: &str) -> NixError {
    let paths = extract_store_paths(text);
    let lower = text.to_lowercase();

    let mut best: Option<&Rule> = None;
    for rule in RULES {
        if !lower_contains_all(&lower, rule.needles) {
            continue;
        }
        if let Some(extra) = rule.extra {
            if !extra(text) {
                continue;
            }
        }
        match best {
            None => best = Some(rule),
            Some(prev) if rule.priority < prev.priority => best = Some(rule),
            Some(_) => {}
        }
    }

    let (kind, summary) = match best {
        Some(rule) => (rule.kind, (rule.summary)(text, &paths)),
        None => {
            // Heuristic: path does not exist without our exact needle pairing.
            if lower.contains("/nix/store/") && lower.contains("does not exist") {
                (
                    NixErrorKind::MissingStorePath,
                    summary_missing_path(text, &paths),
                )
            } else {
                (NixErrorKind::Unknown, summary_unknown(text, &paths))
            }
        }
    };

    NixError {
        kind,
        summary,
        detail: trim_detail(text),
        paths,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::web::nix::errors::NixErrorKind;

    #[test]
    fn classifies_missing_store_path() {
        let sample = r#"
error:
       … while calling the 'toJSON' builtin
         at «string»:1:2:
       error: path '/nix/store/z10yq3qjir82v7jb3nakx5hm3hr0qv9r-source/flake.nix' does not exist
"#;
        let e = classify(sample);
        assert_eq!(e.kind, NixErrorKind::MissingStorePath);
        assert!(e.paths.iter().any(|p| p.contains("z10yq3qjir82v7jb3nakx5hm3hr0qv9r")));
        assert!(e.user_message().contains("missing-store-path"));
    }

    #[test]
    fn classifies_hash_mismatch() {
        let e = classify("error: hash mismatch in fixed-output derivation '/nix/store/abc-foo'");
        assert_eq!(e.kind, NixErrorKind::HashMismatch);
    }

    #[test]
    fn classifies_network() {
        let e = classify("error: unable to download 'https://example.com/foo': HTTP error 503");
        assert_eq!(e.kind, NixErrorKind::NetworkFetchFailed);
    }

    #[test]
    fn classifies_infinite_recursion() {
        let e = classify("error: infinite recursion encountered");
        assert_eq!(e.kind, NixErrorKind::InfiniteRecursion);
    }

    #[test]
    fn classifies_timeout() {
        let e = classify("timeout waiting for marker __NEO_EVAL_1; output so far: ");
        assert_eq!(e.kind, NixErrorKind::Timeout);
    }

    #[test]
    fn unknown_fallback() {
        let e = classify("something completely unexpected from nix");
        assert_eq!(e.kind, NixErrorKind::Unknown);
    }

    #[test]
    fn classifies_sqlite_busy() {
        let e = classify("error: SQLite database is busy");
        assert_eq!(e.kind, NixErrorKind::PermissionDenied);
    }

    #[test]
    fn classifies_dns_failure() {
        let e = classify("error: unable to download 'https://github.com/x': Could not resolve host");
        // "unable to download" matches Network first
        assert_eq!(e.kind, NixErrorKind::NetworkFetchFailed);
    }

    #[test]
    fn classifies_git_file_lock() {
        let e = classify(
            "error: getting status of 'git+file:///home/dev/neo': No such file or directory",
        );
        assert_eq!(e.kind, NixErrorKind::FlakeLockStale);
    }

    #[test]
    fn classifies_cannot_coerce() {
        let e = classify("error: cannot coerce a set to a string");
        assert_eq!(e.kind, NixErrorKind::EvalAssertion);
    }

    #[test]
    fn classifies_missing_flake_attr() {
        let e = classify("error: flake 'github:foo/bar' does not provide attribute 'nixosModules.default'");
        assert_eq!(e.kind, NixErrorKind::EvalAssertion);
    }
}
