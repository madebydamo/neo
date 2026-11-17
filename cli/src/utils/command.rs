//! Shell quoting, process execution, and thin git/nix wrappers.

use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::io::Write;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus, Output, Stdio};

/// `value` when set and non-empty, else `default`.
pub fn non_empty_or(value: Option<String>, default: &str) -> String {
    value
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// `$var` when set and non-empty, else `default`.
pub fn env_or(var: &str, default: &str) -> String {
    non_empty_or(std::env::var(var).ok(), default)
}

/// The sudo binary (kept for the web UI's imports; see [`super::bins`]).
pub use super::bins::sudo as sudo_bin;

/// `sudo -n`: fails at once when a rule is missing, for anything that runs
/// without a terminal to ask on.
pub fn sudo_command() -> Command {
    let mut c = Command::new(sudo_bin());
    c.arg("-n");
    c
}

pub fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() == 0 }
}

/// `program` directly as root, else through `sudo` (`-n` when nobody can answer
/// a password prompt).
pub fn root_command(program: &str, sudo: &str, non_interactive: bool) -> Command {
    if is_root() {
        return Command::new(program);
    }
    let mut c = Command::new(sudo);
    if non_interactive {
        c.arg("-n");
    }
    c.arg(program);
    c
}

/// Ask on the terminal; anything but `yes` is an error.
pub fn confirm(question: &str) -> Result<()> {
    print!("{question} Type yes to continue: ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    if line.trim() != "yes" {
        bail!("aborted");
    }
    Ok(())
}

/// Seconds since the epoch.
pub fn now_secs() -> u64 {
    epoch_secs(std::time::SystemTime::now())
}

/// A file's modification time in seconds since the epoch (0 when unknown).
pub fn mtime_secs(meta: &std::fs::Metadata) -> u64 {
    meta.modified().map(epoch_secs).unwrap_or(0)
}

fn epoch_secs(t: std::time::SystemTime) -> u64 {
    t.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

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
pub fn shell_join(
    program: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> String {
    let program = shell_quote(&program.as_ref().to_string_lossy());
    let args = shell_words(args);
    if args.is_empty() {
        program
    } else {
        format!("{program} {args}")
    }
}

/// Each word quoted for a POSIX shell, space-separated.
pub fn shell_words(words: impl IntoIterator<Item = impl AsRef<OsStr>>) -> String {
    words
        .into_iter()
        .map(|w| shell_quote(&w.as_ref().to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ")
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

/// Print `cmd`, run it on our terminal and return its status and display line.
fn run_shown(cmd: &mut Command) -> Result<(ExitStatus, String)> {
    let display = format_command(cmd);
    println!("→ {display}");
    let status = cmd
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to spawn: {display}"))?;
    // The child shares our stdout; SIGPIPE means its reader went away
    // (`neo … | head`, also through the `sudo -u homeserver` re-exec). Stop
    // quietly like the child did instead of reporting a failure.
    if status.signal() == Some(libc::SIGPIPE) {
        std::process::exit(128 + libc::SIGPIPE);
    }
    Ok((status, display))
}

/// Print `cmd` and run it on our terminal; a non-zero exit is an error.
pub fn execute_command(cmd: &mut Command) -> Result<()> {
    let (status, display) = run_shown(cmd)?;
    if !status.success() {
        match status.code() {
            Some(code) => bail!("Command failed: {display} (exit {code})"),
            None => bail!("Command failed: {display} ({status})"),
        }
    }
    Ok(())
}

/// Like [`execute_command`], for commands whose exit code carries meaning:
/// returns it (`-1` when a signal ended the command) instead of failing.
pub fn status_code(cmd: &mut Command) -> Result<i32> {
    let (status, _) = run_shown(cmd)?;
    Ok(status.code().unwrap_or(-1))
}

/// `out` when the command succeeded, else `{what} failed: {stderr}` (or the
/// exit status when stderr was empty or not captured).
pub fn check_output(out: Output, what: &str) -> Result<Output> {
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            bail!("{what} failed ({})", out.status);
        }
        bail!("{what} failed: {stderr}");
    }
    Ok(out)
}

/// Run `cmd` capturing its output; failing to start it or a non-zero exit is an error.
/// Commands whose exit code carries meaning (not found, merge conflict) check it themselves.
pub fn output_checked(cmd: &mut Command, what: &str) -> Result<Output> {
    let out = cmd
        .output()
        .with_context(|| format!("run {what} (is it installed?)"))?;
    check_output(out, what)
}

/// [`output_checked`], returning stdout as text (not trimmed).
pub fn stdout_checked(cmd: &mut Command, what: &str) -> Result<String> {
    let out = output_checked(cmd, what)?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run `cmd` with `input` on stdin and capture its output. The exit status is
/// left to the caller ([`check_output`]): a command that stops reading early
/// reports why there, not as a broken pipe here.
pub fn output_with_stdin(cmd: &mut Command, input: &[u8], what: &str) -> Result<Output> {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("run {what} (is it installed?)"))?;
    let mut stdin = child.stdin.take().context("stdin of the child")?;
    // From another thread: a child that fills its stdout before reading all of
    // its stdin would otherwise wait for us forever.
    std::thread::scope(|s| {
        s.spawn(move || {
            let _ = stdin.write_all(input);
        });
        child
            .wait_with_output()
            .with_context(|| format!("wait for {what}"))
    })
}

/// `nix` with flakes turned on, whatever the machine's own nix.conf says.
pub fn nix_command(nix_cmd: &str) -> Command {
    let mut c = Command::new(nix_cmd);
    c.args(["--extra-experimental-features", "nix-command flakes"]);
    c
}

/// The attribute `neo build` and `neo activate` build first.
pub const TOPLEVEL_ATTR: &str = ".#nixosConfigurations.neo.config.system.build.toplevel";

pub fn run_nix(config_path: &str, nix_cmd: &str, args: &[&str]) -> Result<()> {
    execute_command(nix_command(nix_cmd).current_dir(config_path).args(args))
}

/// `nix run --impure .#write-flake`.
///
/// The homeserver template decides the neo flake input with `builtins.pathExists`
/// on the laptop checkout. Pure eval hides paths outside the config repo, so a
/// pure write-flake leaves `neo.url` at `github:madebydamo/neo`. `--impure` is a
/// flag of `nix run` and is rejected before the subcommand.
pub fn write_flake_args() -> &'static [&'static str] {
    &["run", "--impure", ".#write-flake"]
}

pub fn run_write_flake(config_path: &str, nix_cmd: &str) -> Result<()> {
    run_nix(config_path, nix_cmd, write_flake_args())
}

pub fn git_cmd(config_path: &str, args: &[&str]) -> Result<()> {
    execute_command(Command::new("git").current_dir(config_path).args(args))
}

/// `epoch` broken down in the host's local time zone.
pub fn local_time(epoch: i64) -> Option<libc::tm> {
    let t = epoch as libc::time_t;
    // SAFETY: zeroed tm is a valid out-parameter; localtime_r is thread-safe.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
    ok.then_some(tm)
}

/// `YYYYMMDD-HHMMSS` of `epoch` in local time (op ids, commit names).
fn timestamp_at(epoch: i64) -> String {
    match local_time(epoch) {
        Some(tm) => format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        ),
        None => epoch.to_string(),
    }
}

/// Now as `YYYYMMDD-HHMMSS` in local time.
pub fn get_timestamp() -> String {
    timestamp_at(i64::try_from(now_secs()).unwrap_or(i64::MAX))
}

/// Whether the index differs from HEAD (`git diff --staged --quiet`).
pub fn has_staged_changes(config_path: &str) -> Result<bool> {
    let status = Command::new("git")
        .current_dir(config_path)
        .args(["diff", "--staged", "--quiet"])
        .status()
        .context("run git diff --staged (is git installed?)")?;
    match status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => bail!("git diff --staged failed in {config_path} ({status})"),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    #[test]
    fn checked_output_reports_stderr() {
        assert_eq!(stdout_checked(&mut sh("echo ok"), "t").unwrap(), "ok\n");
        let e = stdout_checked(&mut sh("echo boom >&2; exit 3"), "thing").unwrap_err();
        assert_eq!(e.to_string(), "thing failed: boom");
        let e = output_checked(&mut Command::new("/nonexistent/x"), "x").unwrap_err();
        assert!(e.to_string().contains("is it installed"));
    }

    #[test]
    fn stdin_is_fed_without_deadlock() {
        // More than a pipe buffer in both directions.
        let input = vec![b'x'; 1 << 20];
        let out = output_with_stdin(&mut sh("cat"), &input, "cat").unwrap();
        assert_eq!(out.stdout.len(), input.len());
        // A child that ignores its stdin only reports its own status.
        let out = output_with_stdin(&mut sh("exit 7"), &input, "t").unwrap();
        assert_eq!(out.status.code(), Some(7));
    }

    #[test]
    fn status_code_returns_the_exit_code() {
        assert_eq!(status_code(&mut sh("exit 0")).unwrap(), 0);
        assert_eq!(status_code(&mut sh("exit 4")).unwrap(), 4);
        assert!(status_code(&mut Command::new("/nonexistent/x")).is_err());
        let e = execute_command(&mut sh("exit 3")).unwrap_err();
        assert!(e.to_string().ends_with("(exit 3)"), "{e}");
    }

    #[test]
    fn check_output_without_stderr_reports_the_status() {
        let e = stdout_checked(sh("exit 5").stderr(Stdio::null()), "thing").unwrap_err();
        assert!(
            e.to_string().starts_with("thing failed (exit status: 5"),
            "{e}"
        );
    }

    #[test]
    fn timestamp_is_local_date_and_time() {
        let ts = get_timestamp();
        assert_eq!(ts.len(), 15, "{ts}");
        assert_eq!(&ts[8..9], "-");
        assert!(ts.chars().filter(|c| *c != '-').all(|c| c.is_ascii_digit()));
        let tm = local_time(0).unwrap();
        let expect = format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        );
        assert_eq!(timestamp_at(0), expect);
    }

    #[test]
    fn staged_changes_need_a_repository() {
        let dir = crate::utils::fsutil::TempDir::new("neo-command-test").unwrap();
        // Outside a git repo `git diff --staged` fails (exit 129): an error, not "no changes".
        assert!(has_staged_changes(&dir.path().to_string_lossy()).is_err());
    }

    #[test]
    fn write_flake_runs_impure_so_path_exists_can_see_the_checkout() {
        // `nix run --impure .#write-flake`. `--impure` before `run` is rejected.
        assert_eq!(write_flake_args(), ["run", "--impure", ".#write-flake"]);
    }
}
