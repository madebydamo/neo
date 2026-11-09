//! JSON state files under neo-web's state directory (proposals, version notes).
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

/// `None` when the file is missing or unreadable: callers start empty.
pub fn load_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Replace `path` with `body` in one step (temp file + rename).
pub fn write_atomic(path: &Path, body: &str, mode: u32) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(&tmp)
        .and_then(|mut f| f.write_all(body.as_bytes()))
        .with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn write_then_load_round_trips_with_the_mode() {
        let dir = std::env::temp_dir().join(format!("neo-state-file-{}", std::process::id()));
        let path = dir.join("sub/state.json");
        write_atomic(&path, "[1,2]", 0o600).unwrap();
        assert_eq!(load_json::<Vec<u32>>(&path), Some(vec![1, 2]));
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_or_broken_file_loads_as_none() {
        let dir = std::env::temp_dir().join(format!("neo-state-file2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        assert_eq!(load_json::<Vec<u32>>(&path), None);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(load_json::<Vec<u32>>(&path), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
