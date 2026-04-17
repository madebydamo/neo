use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn nuke(config_path: &str, dry_run: bool, _nix_cmd: &str) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: would remove all non-dot files/directories in {}",
            config_path
        );
        return Ok(());
    }

    let p = Path::new(config_path);

    if p.exists() {
        let mut to_delete = Vec::new();

        for entry_res in fs::read_dir(p).context("nuke failed (read dir)")? {
            let entry = entry_res.context("nuke failed (read entry)")?;

            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }

            to_delete.push(entry);
        }

        for entry in to_delete {
            let path = entry.path();

            if entry
                .file_type()
                .context("nuke failed (get file type)")?
                .is_dir()
            {
                fs::remove_dir_all(&path).context("nuke failed (remove subdir)")?;
            } else {
                fs::remove_file(&path).context("nuke failed (remove file)")?;
            }
        }
    } else {
        fs::create_dir_all(p).context("recreate config dir after nuke")?;
    }

    println!("Nuked {}", config_path);
    Ok(())
}
