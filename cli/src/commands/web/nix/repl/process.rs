//! Nix repl process helpers: extract files, stderr error detection.
use std::fs;
use std::path::Path;

use anyhow::Result;

use super::super::registry::{EXTRACT_LIB, NIX_EXTRACTORS};

/// True when `text` looks like a finished Nix evaluation error (not a progress line).
pub(super) fn looks_like_terminal_nix_error(text: &str) -> bool {
    for line in text.lines() {
        let t = line.trim_start();
        // `error:` starts evaluation failures; `error: path …` also appears nested in traces.
        if t.starts_with("error:") || t.contains("error: path ") {
            return true;
        }
    }
    false
}

pub(super) fn trim_for_error(s: &str) -> String {
    let t = s.trim();
    if t.len() <= 4000 {
        t.to_string()
    } else {
        // Prefer the end of the trace (leaf error).
        tail_chars(t, 4000)
    }
}

pub(super) fn tail_chars(s: &str, n: usize) -> String {
    s.chars()
        .rev()
        .take(n)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

pub(crate) fn write_extract_files(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    for e in NIX_EXTRACTORS {
        fs::write(dir.join(e.file_name), e.content)?;
    }
    fs::write(dir.join(EXTRACT_LIB.0), EXTRACT_LIB.1)?;
    Ok(())
}
