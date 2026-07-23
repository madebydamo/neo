//! Nix repl process helpers: extract files, config mtime, stderr error detection.
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use anyhow::Result;

use super::super::registry::NIX_EXTRACTORS;

/// True when `text` looks like a finished Nix evaluation error (not a progress line).
pub(super) fn looks_like_terminal_nix_error(text: &str) -> bool {
    for line in text.lines() {
        let t = line.trim_start();
        // Primary form Nix uses for evaluation failures.
        if t.starts_with("error:") {
            return true;
        }
        // Nested / secondary form in multi-line traces.
        if t.starts_with("error: ") || t.contains("error: path ") {
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

pub(super) fn write_extract_files(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    for e in NIX_EXTRACTORS {
        fs::write(dir.join(e.file_name), e.content)?;
    }
    Ok(())
}

pub(super) fn current_config_mtime(config_dir: &str) -> SystemTime {
    let root = Path::new(config_dir);
    let mut max_t = SystemTime::UNIX_EPOCH;
    if let Ok(meta) = fs::metadata(root) {
        if let Ok(t) = meta.modified() {
            let root_relevant = meta.is_dir()
                || root
                    .extension()
                    .and_then(|e| e.to_str())
                    .map_or(false, |e| e == "nix" || e == "toml" || e == "lock");
            if root_relevant && t > max_t {
                max_t = t;
            }
        }
        if meta.is_dir() {
            if let Ok(rd) = fs::read_dir(root) {
                for entry in rd.flatten() {
                    walk_mtime(&entry.path(), &mut max_t);
                }
            }
        }
    }
    max_t
}

fn walk_mtime(p: &Path, max_t: &mut SystemTime) {
    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.') || name == "result" || name == "target" {
            return;
        }
    }
    let meta = match fs::metadata(p) {
        Ok(m) => m,
        Err(_) => return,
    };
    if let Ok(t) = meta.modified() {
        let relevant = meta.is_dir()
            || p.extension()
                .and_then(|e| e.to_str())
                .map_or(false, |e| e == "nix" || e == "toml" || e == "lock");
        if relevant && t > *max_t {
            *max_t = t;
        }
    }
    if meta.is_dir() {
        if let Ok(rd) = fs::read_dir(p) {
            for entry in rd.flatten() {
                walk_mtime(&entry.path(), max_t);
            }
        }
    }
}
