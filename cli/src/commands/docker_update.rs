// docker-update <container> : pulls the container's current image (via inspect) and restarts its docker-*.service.
// Useful from shell / just exec; web UI uses direct handlers too.
use anyhow::Result;
use std::process::Command;

use crate::utils::command::stdout_checked;
use crate::utils::execute_command;

pub fn docker_update(container: &str, sudo_cmd: &str) -> Result<()> {
    let cname = container.strip_prefix("docker-").unwrap_or(container);
    println!("→ docker-update for container {}", cname);

    // Inspect the running image ref (resolves current tag/digest for :latest etc)
    let img = stdout_checked(
        Command::new("docker").args(["inspect", "--format", "{{.Config.Image}}", cname]),
        "docker inspect",
    )?
    .trim()
    .to_string();
    if img.is_empty() {
        anyhow::bail!("no image found for container {}", cname);
    }
    println!("image: {}", img);

    execute_command(Command::new("docker").args(["pull", &img]))?;

    let unit = format!("docker-{}", cname);
    execute_command(Command::new(sudo_cmd).args([
        "systemctl",
        "restart",
        &unit,
        "--no-ask-password",
    ]))?;

    println!("✓ docker-update complete for {}", cname);
    Ok(())
}
