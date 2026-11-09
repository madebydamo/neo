//! Import and export working-tree `settings.toml` (web upload/download).

use std::fs;
use std::path::Path;

use toml_edit::DocumentMut;

/// Parse an uploaded `settings.toml`. Writing it goes through `apply_edit`.
pub fn parse_settings_upload(raw: &str) -> Result<DocumentMut, String> {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    if raw.trim().is_empty() {
        return Err("Uploaded file is empty".into());
    }
    let doc: DocumentMut = raw
        .parse()
        .map_err(|e: toml_edit::TomlError| format!("Invalid TOML: {e}"))?;
    if doc.as_table().is_empty() {
        return Err("Uploaded settings.toml has no keys".into());
    }
    Ok(doc)
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
    fn upload_rejects_invalid_toml() {
        let err = parse_settings_upload("not = [toml").unwrap_err();
        assert!(err.contains("Invalid TOML"), "unexpected error: {err}");
    }

    #[test]
    fn upload_rejects_empty_payload() {
        let err = parse_settings_upload("  \n\t").unwrap_err();
        assert!(err.contains("empty"), "unexpected error: {err}");
    }

    #[test]
    fn upload_rejects_document_with_no_keys() {
        let err = parse_settings_upload("# comments only\n").unwrap_err();
        assert!(err.contains("no keys"), "unexpected error: {err}");
    }

    #[test]
    fn upload_strips_utf8_bom() {
        let doc = parse_settings_upload("\u{feff}core.hostname = \"neo\"\n").unwrap();
        assert!(doc.to_string().contains("hostname"));
        assert!(!doc.to_string().starts_with('\u{feff}'));
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
