//! `neo secrets …`: secret settings outside settings.toml (docs/design/secrets.md).
//!
//! `split` moves every secret option value from settings.toml into secrets.yaml
//! (plain, the unencrypted backend). Saves in the web UI keep routing secrets
//! there from then on.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;
use toml_edit::DocumentMut;

use crate::commands::web::nix::registry::{EXTRACT_LIB, EXTRACT_SECRET_PATHS};
use crate::utils::secrets::{self, Backend};
use crate::utils::{git_cmd, sort_document_alphabetically};

/// Secret option paths of the config flake at `config_path` ("*" = any attrsOf key).
pub fn secret_paths(config_path: &str, nix_cmd: &str) -> Result<Vec<Vec<String>>> {
    #[derive(serde::Deserialize)]
    struct RawPaths {
        paths: Vec<Vec<String>>,
    }
    let dir = std::env::temp_dir().join(format!("neo-secret-paths-{}", std::process::id()));
    fs::create_dir_all(&dir).context("temp dir for the secret path extractor")?;
    fs::write(dir.join(EXTRACT_LIB.0), EXTRACT_LIB.1)?;
    let extractor = dir.join(EXTRACT_SECRET_PATHS.file_name);
    fs::write(&extractor, EXTRACT_SECRET_PATHS.content)?;
    let repo = fs::canonicalize(config_path).with_context(|| format!("resolve {config_path}"))?;
    let expr = format!(
        "import {} {{ neoFlake = \"git+file://{}\"; }}",
        extractor.display(),
        repo.display()
    );
    let out = Command::new(nix_cmd)
        .args(["--extra-experimental-features", "nix-command flakes"])
        .args(["eval", "--json", "--impure", "--expr", &expr])
        .output()
        .context("run nix eval for secret paths")?;
    let _ = fs::remove_dir_all(&dir);
    if !out.status.success() {
        bail!(
            "nix eval of secret paths failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let raw: RawPaths =
        serde_json::from_slice(&out.stdout).context("parse secret paths from nix eval")?;
    Ok(raw.paths)
}

pub fn split(config_path: &str, dry_run: bool, nix_cmd: &str) -> Result<()> {
    let settings_path = Path::new(config_path).join("settings.toml");
    let plain_path = Path::new(config_path).join(secrets::PLAIN_FILE);
    let mut tree = match secrets::backend_for(&settings_path) {
        Backend::Sops(p) => bail!(
            "{} exists: secrets are already encrypted, nothing to split",
            p.display()
        ),
        Backend::Plain(p) => secrets::read_plain(&p)?,
        Backend::Inline => secrets::Tree::new(),
    };
    let patterns = secret_paths(config_path, nix_cmd)?;
    let content = fs::read_to_string(&settings_path)
        .with_context(|| format!("read {}", settings_path.display()))?;
    let mut doc: DocumentMut = content.parse().context("parse settings.toml")?;
    let split = secrets::split_into(&mut doc, &mut tree, &patterns, None);
    if split.moved.is_empty() && plain_path.exists() {
        println!("No secret settings left in settings.toml");
        return Ok(());
    }
    for p in &split.moved {
        println!("  {} → {}", p.join("."), secrets::PLAIN_FILE);
    }
    if dry_run {
        println!(
            "DRY-RUN: would write {} and settings.toml",
            plain_path.display()
        );
        return Ok(());
    }
    // Secrets first: if settings.toml fails to write, the values still exist
    // in both files and the secrets file wins on evaluation.
    secrets::write_plain(&plain_path, &tree)?;
    sort_document_alphabetically(&mut doc);
    fs::write(&settings_path, doc.to_string()).context("write settings.toml")?;
    git_cmd(config_path, &["add", secrets::PLAIN_FILE, "settings.toml"])?;
    println!(
        "Moved {} secret setting(s) to {}",
        split.moved.len(),
        plain_path.display()
    );
    Ok(())
}
