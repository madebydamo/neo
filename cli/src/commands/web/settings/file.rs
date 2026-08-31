//! Import and export working-tree `settings.toml` (web upload/download).

use std::fs;
use std::path::Path;

use toml_edit::DocumentMut;

use crate::utils::sort_document_alphabetically;

/// Replace `settings.toml` with uploaded TOML. Parses and sorts; does not write on error.
pub fn import_settings_toml(path: &Path, raw: &str) -> Result<(), String> {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    if raw.trim().is_empty() {
        return Err("Uploaded file is empty".into());
    }
    let mut doc: DocumentMut = raw
        .parse()
        .map_err(|e: toml_edit::TomlError| format!("Invalid TOML: {e}"))?;
    if doc.as_table().is_empty() {
        return Err("Uploaded settings.toml has no keys".into());
    }
    sort_document_alphabetically(&mut doc);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create directory: {e}"))?;
    }
    fs::write(path, doc.to_string()).map_err(|e| format!("write settings.toml: {e}"))?;
    Ok(())
}

/// Read working-tree `settings.toml` for download.
pub fn export_settings_toml(path: &Path) -> Result<String, String> {
    if !path.is_file() {
        return Err(format!("settings.toml not found at {}", path.display()));
    }
    fs::read_to_string(path).map_err(|e| format!("read settings.toml: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn temp_settings() -> std::path::PathBuf {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("neo-settings-file-{}-{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        dir.join("settings.toml")
    }

    #[test]
    fn import_rejects_invalid_toml_and_leaves_existing_file() {
        let path = temp_settings();
        fs::write(&path, "keep = true\n").unwrap();
        let err = import_settings_toml(&path, "not = [toml").unwrap_err();
        assert!(
            err.to_lowercase().contains("toml") || err.to_lowercase().contains("invalid"),
            "unexpected error: {err}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "keep = true\n");
    }

    #[test]
    fn import_rejects_empty_payload() {
        let path = temp_settings();
        fs::write(&path, "keep = true\n").unwrap();
        let err = import_settings_toml(&path, "  \n\t").unwrap_err();
        assert!(
            err.to_lowercase().contains("empty"),
            "unexpected error: {err}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "keep = true\n");
    }

    #[test]
    fn import_rejects_document_with_no_keys() {
        let path = temp_settings();
        fs::write(&path, "keep = true\n").unwrap();
        let err = import_settings_toml(&path, "# comments only\n").unwrap_err();
        assert!(
            err.to_lowercase().contains("no keys") || err.to_lowercase().contains("empty"),
            "unexpected error: {err}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "keep = true\n");
    }

    #[test]
    fn import_writes_sorted_toml() {
        let path = temp_settings();
        import_settings_toml(
            &path,
            r#"
[services.zeta]
enabled = true

[services.alpha]
enabled = false
"#,
        )
        .unwrap();
        let written = fs::read_to_string(&path).unwrap();
        let alpha = written.find("[services.alpha]").expect("alpha table");
        let zeta = written.find("[services.zeta]").expect("zeta table");
        assert!(
            alpha < zeta,
            "expected alphabetical table order:\n{written}"
        );
    }

    #[test]
    fn import_strips_utf8_bom() {
        let path = temp_settings();
        import_settings_toml(&path, "\u{feff}core.hostname = \"neo\"\n").unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("hostname"));
        assert!(!written.starts_with('\u{feff}'));
    }

    #[test]
    fn export_reads_existing_file() {
        let path = temp_settings();
        fs::write(&path, "hello = 1\n").unwrap();
        assert_eq!(export_settings_toml(&path).unwrap(), "hello = 1\n");
    }

    #[test]
    fn export_errors_when_missing() {
        let path = temp_settings();
        let _ = fs::remove_file(&path);
        let err = export_settings_toml(&path).unwrap_err();
        assert!(
            err.contains("not found") || err.to_lowercase().contains("no such"),
            "unexpected error: {err}"
        );
    }
}
