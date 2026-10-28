//! `neo secrets …`: secret settings outside settings.toml (docs/design/secrets.md).
//!
//!   split    settings.toml → secrets.yaml (plain)
//!   enable   → secrets.sops.yaml for the host + core.ssh.authorizedKeys (ssh-ed25519),
//!            host key escrowed for the admins in identity.sops.yaml
//!   disable  secrets.sops.yaml → secrets.yaml
//!   rekey    re-encrypt for the current keys
//! The web UI Secrets card runs the same operations (utils/secrets_store.rs).

use anyhow::{bail, Context, Result};
use std::fs;
use std::process::Command;

use crate::commands::web::nix::registry::{EXTRACT_LIB, EXTRACT_SECRET_PATHS};
use crate::utils::secrets::{self, Backend};
use crate::utils::secrets_store::{Store, IDENTITY_FILE};

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
    let store = Store::new(config_path);
    if let Backend::Sops(p) = store.backend() {
        bail!("{} exists: secrets are already encrypted", p.display());
    }
    let patterns = secret_paths(config_path, nix_cmd)?;
    if dry_run {
        println!(
            "DRY-RUN: would move secret settings into {}",
            secrets::PLAIN_FILE
        );
        return Ok(());
    }
    let moved = store.split(&patterns)?;
    for p in &moved {
        println!("  {} → {}", p.join("."), secrets::PLAIN_FILE);
    }
    if moved.is_empty() {
        println!("No secret settings left in settings.toml");
    } else {
        println!("Moved {} secret setting(s); activate to apply", moved.len());
    }
    Ok(())
}

pub fn enable(config_path: &str, dry_run: bool, nix_cmd: &str) -> Result<()> {
    let store = Store::new(config_path);
    let patterns = secret_paths(config_path, nix_cmd)?;
    if dry_run {
        println!("DRY-RUN: would encrypt secrets into {}", secrets::SOPS_FILE);
        return Ok(());
    }
    let r = store.enable(&patterns)?;
    println!(
        "Encrypted {} secret(s) for {} recipient(s) into {}",
        r.values,
        r.recipients,
        secrets::SOPS_FILE
    );
    if r.escrowed {
        println!("Host key escrowed for the admins in {IDENTITY_FILE}");
    } else {
        println!("Host key not escrowed (run on the machine to write {IDENTITY_FILE})");
    }
    println!("Activate to apply.");
    Ok(())
}

pub fn disable(config_path: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: would decrypt {} into {}",
            secrets::SOPS_FILE,
            secrets::PLAIN_FILE
        );
        return Ok(());
    }
    let n = Store::new(config_path).disable()?;
    println!(
        "Decrypted {n} secret(s) into {}; activate to apply",
        secrets::PLAIN_FILE
    );
    Ok(())
}

pub fn rekey(config_path: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: would re-encrypt {}", secrets::SOPS_FILE);
        return Ok(());
    }
    let n = Store::new(config_path).rekey()?;
    println!("Re-encrypted for {n} recipient(s)");
    Ok(())
}
