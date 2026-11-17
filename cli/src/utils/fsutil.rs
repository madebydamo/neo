//! Atomic and private files, temp folders.

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

/// Replace `path` with `content` in one step: a temp file in the same folder,
/// synced, then renamed over `path`, so a reader never sees half a file.
///
/// `mode` is set on the temp file before the first byte is written (a secret
/// is never readable with the umask's permissions). `None` keeps the mode of
/// the file being replaced, or the umask default for a new file.
pub fn write_atomic(path: &Path, content: &[u8], mode: Option<u32>) -> Result<()> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let name = path
        .file_name()
        .with_context(|| format!("no file name in {}", path.display()))?
        .to_string_lossy();
    let mode = mode.or_else(|| {
        fs::metadata(path)
            .ok()
            .map(|m| m.permissions().mode() & 0o7777)
    });
    let (tmp, mut f) = loop {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let tmp = dir.join(format!(".{name}.{}-{n}.tmp", std::process::id()));
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        if let Some(m) = mode {
            opts.mode(m & 0o777);
        }
        match opts.open(&tmp) {
            Ok(f) => break (tmp, f),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).with_context(|| format!("write {}", tmp.display())),
        }
    };
    let written = (|| -> io::Result<()> {
        if let Some(m) = mode {
            // `mode` above is masked by the umask; set it exactly.
            f.set_permissions(fs::Permissions::from_mode(m))?;
        }
        f.write_all(content)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("write {}", path.display()));
    }
    // Make the rename itself durable; a failure here loses nothing readers see.
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
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
    fn write_atomic_replaces_and_leaves_no_temp_file() {
        let dir = TempDir::new("neo-fsutil-test").unwrap();
        let f = dir.join("state.json");
        write_atomic(&f, b"one", Some(0o600)).unwrap();
        assert_eq!(mode(&f), 0o600);
        write_atomic(&f, b"two", None).unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"two");
        // None keeps the replaced file's mode.
        assert_eq!(mode(&f), 0o600);
        fs::set_permissions(&f, fs::Permissions::from_mode(0o640)).unwrap();
        write_atomic(&f, b"three", None).unwrap();
        assert_eq!(mode(&f), 0o640);
        write_atomic(&f, b"four", Some(0o644)).unwrap();
        assert_eq!(mode(&f), 0o644);
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("state.json")]);
    }

    #[test]
    fn write_atomic_fails_without_the_folder() {
        let dir = TempDir::new("neo-fsutil-test").unwrap();
        assert!(write_atomic(&dir.join("missing/f"), b"x", None).is_err());
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
