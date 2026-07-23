// docker-update <container> : pulls the container's current image (via inspect) and restarts its docker-*.service.
// Useful from shell / just exec; web UI uses direct handlers too.
use anyhow::{Context, Result};
use std::process::Command;

use crate::utils::execute_command;

pub fn docker_update(container: &str) -> Result<()> {
    let cname = if container.starts_with("docker-") {
        &container[7..]
    } else {
        container
    };
    println!("→ docker-update for container {}", cname);

    // Inspect the running image ref (resolves current tag/digest for :latest etc)
    let inspect = Command::new("docker")
        .args(["inspect", "--format", "{{.Config.Image}}", cname])
        .output()
        .context("docker inspect")?;
    let img = String::from_utf8_lossy(&inspect.stdout).trim().to_string();
    if img.is_empty() {
        anyhow::bail!("no image found for container {}", cname);
    }
    println!("image: {}", img);

    execute_command(&mut Command::new("docker").args(["pull", &img]))?;

    let unit = format!("docker-{}", cname);
    execute_command(&mut Command::new("sudo").args([
        "systemctl",
        "restart",
        &unit,
        "--no-ask-password",
    ]))?;

    println!("✓ docker-update complete for {}", cname);
    Ok(())
}
