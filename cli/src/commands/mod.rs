pub mod activate;
pub mod build;
pub mod docker_update;
pub mod edit;
pub mod generate_hardware;
pub mod git;
pub mod init;
pub mod log;
pub mod migrate;
pub mod nuke;
pub mod paste_settings;
pub mod profile;
pub mod toml_sort;
pub mod update;
pub mod update_inputs;
pub mod web;

use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Quote a string for safe copy-paste into a POSIX shell (bash/sh).
///
/// Bare tokens stay unquoted when they only use safe characters; otherwise
/// single quotes are used (with proper escaping of embedded `'`).
pub fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    // Safe unquoted: alphanumerics + common path/flag/flake chars.
    // `#` is fine mid-token (e.g. `.#write-flake`); it only comments at word start.
    let safe = s.bytes().all(|b| {
        matches!(
            b,
            b'a'..=b'z'
                | b'A'..=b'Z'
                | b'0'..=b'9'
                | b'-'
                | b'_'
                | b'.'
                | b'/'
                | b':'
                | b'='
                | b'+'
                | b'@'
                | b'%'
                | b','
                | b'#'
        )
    });
    if safe {
        s.to_string()
    } else if !s.contains('\'') {
        format!("'{s}'")
    } else {
        // bash: 'foo'\''bar' → foo'bar
        let mut out = String::with_capacity(s.len() + 8);
        out.push('\'');
        for ch in s.chars() {
            if ch == '\'' {
                out.push_str("'\\''");
            } else {
                out.push(ch);
            }
        }
        out.push('\'');
        out
    }
}

/// Join program + args into a shell-copyable command line (no cwd).
pub fn shell_join(program: impl AsRef<OsStr>, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> String {
    let mut parts = Vec::new();
    parts.push(shell_quote(&program.as_ref().to_string_lossy()));
    for a in args {
        parts.push(shell_quote(&a.as_ref().to_string_lossy()));
    }
    parts.join(" ")
}

/// Format a `Command` for display: prompt-style cwd, then shell-copyable argv.
///
/// Example: `/var/neo/config $ nix run .#write-flake`
/// Copy everything after `$ ` to re-run (in that directory).
pub fn format_command(cmd: &Command) -> String {
    let cmdline = shell_join(cmd.get_program(), cmd.get_args());
    match cmd.get_current_dir() {
        Some(dir) => format!("{} $ {}", dir.display(), cmdline),
        None => format!("$ {cmdline}"),
    }
}

pub fn execute_command(cmd: &mut Command) -> Result<()> {
    let display = format_command(cmd);
    println!("→ {display}");
    let status = cmd
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to spawn: {display}"))?;
    if !status.success() {
        anyhow::bail!("Command failed: {display}");
    }
    Ok(())
}

pub fn run_nix(config_path: &str, nix_cmd: &str, args: &[&str]) -> Result<()> {
    execute_command(
        Command::new(nix_cmd)
            .current_dir(config_path)
            .args(["--extra-experimental-features", "nix-command flakes"])
            .args(args),
    )
}

pub fn git_cmd(config_path: &str, args: &[&str]) -> Result<()> {
    execute_command(Command::new("git").current_dir(config_path).args(args))
}

pub fn get_timestamp() -> String {
    Command::new("date")
        .arg("+%Y%m%d-%H%M%S")
        .output()
        .map(|o| {
            std::string::String::from_utf8_lossy(&o.stdout)
                .trim()
                .to_string()
        })
        .unwrap_or_else(|_| "ts".to_string())
}

pub fn has_staged_changes(config_path: &str) -> bool {
    Command::new("git")
        .current_dir(config_path)
        .args(["diff", "--staged", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false)
}

pub fn get_current_branch(config_path: &str) -> Result<String> {
    let out = Command::new("git")
        .current_dir(config_path)
        .args(["branch", "--show-current"])
        .output()
        .context("get current branch")?;
    let name = std::string::String::from_utf8_lossy(&out.stdout)
        .trim()
        .to_string();
    if !name.is_empty() {
        Ok(name)
    } else {
        let out = Command::new("git")
            .current_dir(config_path)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .context("get abbrev ref")?;
        let n = std::string::String::from_utf8_lossy(&out.stdout)
            .trim()
            .to_string();
        Ok(if n.is_empty() { "HEAD".to_string() } else { n })
    }
}

pub fn run_nix_logged(
    config_path: &str,
    nix_cmd: &str,
    args: &[&str],
    log_path: &Path,
) -> Result<()> {
    let mut cmd = Command::new(nix_cmd);
    cmd.current_dir(config_path)
        .args(["--extra-experimental-features", "nix-command flakes"])
        .args(args);
    let display = format_command(&cmd);
    let output = cmd
        .output()
        .with_context(|| format!("failed to run nix logged in {config_path}"))?;
    let combined = format!(
        "{display}\nstdout:\n{}\nstderr:\n{}\nexit: {:?}\n\n",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        output.status.code()
    );
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    f.write_all(combined.as_bytes())?;
    if !output.status.success() {
        anyhow::bail!("nix command failed (see log)");
    }
    Ok(())
}
