// Classify raw Nix evaluator / stderr text into stable error kinds.
mod rules;

use super::{NixError, NixErrorKind};
use rules::{summary_missing_path, summary_unknown, Rule, RULES};

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
    needles
        .iter()
        .all(|n| text_lower.contains(&n.to_lowercase()))
}

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
        assert!(e
            .paths
            .iter()
            .any(|p| p.contains("z10yq3qjir82v7jb3nakx5hm3hr0qv9r")));
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
        let e =
            classify("error: unable to download 'https://github.com/x': Could not resolve host");
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
        let e = classify(
            "error: flake 'github:foo/bar' does not provide attribute 'nixosModules.default'",
        );
        assert_eq!(e.kind, NixErrorKind::EvalAssertion);
    }
}
