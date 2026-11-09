//! Private files and temp folders.

use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Write a file only its owner can read, from the first byte on (a key must
/// never exist with the umask's permissions, even briefly).
pub fn write_private(path: &Path, content: &[u8]) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("write {}", path.display()))?;
    // `mode` only applies to a new file.
    f.set_permissions(fs::Permissions::from_mode(0o600))?;
    f.write_all(content)
        .with_context(|| format!("write {}", path.display()))
}

/// A new folder only its owner can enter, removed when dropped (also on `?`).
pub struct TempDir(PathBuf);

impl TempDir {
    /// `<temp dir>/<prefix>-<pid>-<n>`. An existing folder of that name is never
    /// reused: the temp dir is world-writable, so it could be someone else's.
    pub fn new(prefix: &str) -> Result<Self> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        loop {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("{prefix}-{}-{n}", std::process::id()));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(e).with_context(|| format!("create {}", path.display()));
                }
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, name: impl AsRef<Path>) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(p: &Path) -> u32 {
        fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn temp_dir_is_private_unique_and_removed() {
        let a = TempDir::new("neo-fsutil-test").unwrap();
        let b = TempDir::new("neo-fsutil-test").unwrap();
        assert_ne!(a.path(), b.path());
        assert_eq!(mode(a.path()), 0o700);
        fs::write(a.join("f"), "x").unwrap();
        let p = a.path().to_path_buf();
        drop(a);
        assert!(!p.exists());
    }

    #[test]
    fn write_private_tightens_an_existing_file() {
        let dir = TempDir::new("neo-fsutil-test").unwrap();
        let f = dir.join("key");
        write_private(&f, b"secret").unwrap();
        assert_eq!(mode(&f), 0o600);
        fs::set_permissions(&f, fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&f, b"s").unwrap();
        assert_eq!(mode(&f), 0o600);
        assert_eq!(fs::read(&f).unwrap(), b"s");
    }
}
