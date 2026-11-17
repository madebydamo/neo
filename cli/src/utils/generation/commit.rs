//! The generation in activation commit messages (`(generation N)`).

use anyhow::{bail, Context, Result};
use std::process::Command;

use super::current_generation_number;
use crate::utils::command::execute_command;

/// Build activation commit subject including optional generation.
/// Example: `Activation: activation_20260721-123033 (generation 221)`
fn activation_commit_message(activation_branch: &str, generation: Option<u64>) -> String {
    match generation {
        Some(n) => format!("Activation: {activation_branch} (generation {n})"),
        None => format!("Activation: {activation_branch}"),
    }
}

/// Parse `Neo-Generation` / `(generation N)` from a commit subject or body.
pub fn parse_generation_from_message(text: &str) -> Option<u64> {
    // Prefer explicit trailer
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line
            .strip_prefix("Neo-Generation:")
            .or_else(|| line.strip_prefix("neo-generation:"))
        {
            if let Ok(n) = rest.trim().parse::<u64>() {
                if n > 0 {
                    return Some(n);
                }
            }
        }
    }
    // Subject form: `(generation 221)`
    let lower = text.to_ascii_lowercase();
    if let Some(idx) = lower.find("(generation ") {
        let rest = &text[idx + "(generation ".len()..];
        let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = num.parse::<u64>() {
            if n > 0 {
                return Some(n);
            }
        }
    }
    None
}

/// After a successful rebuild, record generation on the activation commit (message only).
/// - If `has_activation_commit` (we created/amended an Activation commit this run): amend message.
/// - Else: empty commit so re-activates of the same tree still get a history node with gen.
pub fn record_generation_in_commit(
    config_path: &str,
    activation_branch: &str,
    has_activation_commit: bool,
) -> Result<u64> {
    let Some(gen) = current_generation_number() else {
        bail!("could not resolve current system generation");
    };
    let msg = activation_commit_message(activation_branch, Some(gen));
    let mut cmd = Command::new("git");
    cmd.current_dir(config_path).arg("commit");
    if has_activation_commit {
        cmd.args(["--amend", "-m", &msg]);
    } else {
        // Same config re-activated, tree unchanged — still record gen on a new history node.
        cmd.args(["--allow-empty", "-m", &msg]);
    }
    execute_command(&mut cmd).context("record the generation in the commit")?;
    Ok(gen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_generation_from_subject() {
        assert_eq!(
            parse_generation_from_message(
                "Activation: activation_20260721-123033 (generation 221)"
            ),
            Some(221)
        );
        assert_eq!(
            parse_generation_from_message("Activation: foo\n\nNeo-Generation: 42\n"),
            Some(42)
        );
        assert_eq!(parse_generation_from_message("Activation: foo"), None);
    }

    #[test]
    fn activation_message_format() {
        assert_eq!(
            activation_commit_message("activation_x", Some(9)),
            "Activation: activation_x (generation 9)"
        );
    }
}
