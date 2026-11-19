//! Terminal questions for `neo bootstrap`. Every question has a default (a
//! flag, the answer from the last run, or a suggestion); Enter keeps it. With
//! `--defaults` nothing is asked and a missing answer is an error.

use anyhow::{bail, Context, Result};
use std::io::{self, BufRead, Write};

pub struct Prompt {
    /// Take every default without asking.
    pub auto: bool,
}

impl Prompt {
    fn read_line(&self) -> Result<String> {
        let mut line = String::new();
        let n = io::stdin().lock().read_line(&mut line)?;
        if n == 0 {
            bail!("no input (stdin closed); pass --defaults with the answers as flags");
        }
        Ok(line.trim().to_string())
    }

    /// A non-empty answer; `check` refuses bad ones with a reason.
    pub fn text(
        &self,
        label: &str,
        default: Option<&str>,
        check: impl Fn(&str) -> Result<(), String>,
    ) -> Result<String> {
        let default = default.filter(|d| !d.is_empty());
        if self.auto {
            let v = default.with_context(|| format!("{label}: no answer (pass it as a flag)"))?;
            check(v).map_err(|e| anyhow::anyhow!("{label}: {e}"))?;
            println!("{label}: {v}");
            return Ok(v.to_string());
        }
        loop {
            match default {
                Some(d) => print!("{label} [{d}]: "),
                None => print!("{label}: "),
            }
            io::stdout().flush()?;
            let line = self.read_line()?;
            let v = if line.is_empty() {
                match default {
                    Some(d) => d.to_string(),
                    None => continue,
                }
            } else {
                line
            };
            match check(&v) {
                Ok(()) => return Ok(v),
                Err(e) => println!("  ! {e}"),
            }
        }
    }

    pub fn yes_no(&self, label: &str, default: bool) -> Result<bool> {
        if self.auto {
            return Ok(default);
        }
        loop {
            print!("{label} [{}]: ", if default { "Y/n" } else { "y/N" });
            io::stdout().flush()?;
            match self.read_line()?.to_lowercase().as_str() {
                "" => return Ok(default),
                "y" | "yes" => return Ok(true),
                "n" | "no" => return Ok(false),
                _ => println!("  ! answer y or n"),
            }
        }
    }

    /// One of `options` (index); Enter takes `default`.
    pub fn choose(&self, label: &str, options: &[String], default: usize) -> Result<usize> {
        if options.is_empty() {
            bail!("{label}: nothing to choose from");
        }
        if self.auto {
            println!("{label}: {}", options[default]);
            return Ok(default);
        }
        println!("{label}:");
        for (i, o) in options.iter().enumerate() {
            let mark = if i == default { "*" } else { " " };
            println!("  {mark}{}) {o}", i + 1);
        }
        loop {
            print!("Choose 1-{} [{}]: ", options.len(), default + 1);
            io::stdout().flush()?;
            let line = self.read_line()?;
            if line.is_empty() {
                return Ok(default);
            }
            match line.parse::<usize>() {
                Ok(n) if (1..=options.len()).contains(&n) => return Ok(n - 1),
                _ => println!("  ! enter a number from the list"),
            }
        }
    }

    /// A password typed twice without echo. Empty is allowed when `optional`.
    pub fn password(&self, label: &str, optional: bool) -> Result<Option<String>> {
        if self.auto {
            if optional {
                return Ok(None);
            }
            bail!("{label}: cannot ask for a password with --defaults");
        }
        loop {
            let first = read_hidden(&format!("{label}: "))?;
            if first.is_empty() {
                if optional {
                    return Ok(None);
                }
                println!("  ! the password must not be empty");
                continue;
            }
            if first.len() < 8 {
                println!("  ! use at least 8 characters");
                continue;
            }
            let again = read_hidden("Repeat it: ")?;
            if again == first {
                return Ok(Some(first));
            }
            println!("  ! the two passwords differ, try again");
        }
    }
}

/// One line from the terminal with echo off (restored on every path).
fn read_hidden(label: &str) -> Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let fd = libc::STDIN_FILENO;
    // SAFETY: termios is plain data; tcgetattr fills it or fails (not a tty).
    let mut term: libc::termios = unsafe { std::mem::zeroed() };
    let tty = unsafe { libc::tcgetattr(fd, &mut term) } == 0;
    let saved = term;
    if tty {
        term.c_lflag &= !libc::ECHO;
        term.c_lflag |= libc::ECHONL;
        // SAFETY: fd is stdin and term came from tcgetattr.
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &term) };
    }
    let mut line = String::new();
    let read = io::stdin().lock().read_line(&mut line);
    if tty {
        // SAFETY: restores the settings read above.
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &saved) };
    }
    if read? == 0 {
        bail!("no input (stdin closed)");
    }
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}
