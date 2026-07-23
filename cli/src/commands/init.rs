use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

use crate::utils::{git_cmd, has_staged_changes, neo_cli_get, run_nix};

pub fn init(
    config_path: &str,
    config: &DocumentMut,
    profile: &str,
    dry_run: bool,
    nix_cmd: &str,
) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: smart init at {} (profile={})",
            config_path, profile
        );
        let repo_url = neo_cli_get(config, profile, "repoUrl").filter(|s| !s.is_empty());
        let bootstrap_method =
            neo_cli_get(config, profile, "bootstrapMethod").unwrap_or("template");
        let template =
            neo_cli_get(config, profile, "template").unwrap_or("github:madebydamo/neo#homeserver");
        let git_user_name = neo_cli_get(config, profile, "gitUserName").unwrap_or("Neo Bootstrap");
        let git_user_email = neo_cli_get(config, profile, "gitUserEmail").unwrap_or("neo@local");
        let default_branch = neo_cli_get(config, profile, "defaultBranch").unwrap_or("master");
        println!("  bootstrapMethod: {}", bootstrap_method);
        if let Some(url) = repo_url {
            println!("  repoUrl: {}", url);
        }
        println!("  template: {}", template);
        println!("  gitUserName: {}", git_user_name);
        println!("  gitUserEmail: {}", git_user_email);
        println!("  defaultBranch: {}", default_branch);
        println!("  Actions: mkdir, smart-git-bootstrap (clone/template+git-init+remote if applicable), set-git-config, generate_hardware, paste_settings, update_inputs, git-add+commit");
        return Ok(());
    }
    fs::create_dir_all(config_path).context("create config dir")?;
    let repo_path = Path::new(config_path);

    // Smart init: handle existing folder gracefully (matches Bash exactly)
    let has_git = repo_path.join(".git").exists();
    let has_flake = repo_path.join("flake.nix").exists();

    if has_git {
        println!("✓ Git repository already exists at {}", config_path);
        println!("  (re-running setup steps — safe even if the worktree is dirty)");
    } else {
        if repo_path
            .read_dir()?
            .any(|e| e.is_ok() && !e.unwrap().file_name().to_string_lossy().starts_with('.'))
        {
            anyhow::bail!("Error: {} is not empty and is not a git repository. Please remove the files first or use a different directory.", config_path);
        }
        println!("→ Initializing new repository at {}...", config_path);
    }

    if !has_flake {
        let repo_url = neo_cli_get(config, profile, "repoUrl").filter(|s| !s.is_empty());

        let bootstrap_method =
            neo_cli_get(config, profile, "bootstrapMethod").unwrap_or("template");

        if let (Some(url), "clone") = (repo_url, bootstrap_method) {
            git_cmd(config_path, &["clone", url, "."])?;
        } else {
            let template = neo_cli_get(config, profile, "template")
                .unwrap_or("github:madebydamo/neo#homeserver");

            run_nix(config_path, nix_cmd, &["flake", "init", "-t", template])?;
            git_cmd(config_path, &["init"])?;

            if let Some(url) = repo_url {
                git_cmd(config_path, &["remote", "add", "origin", url])?;
            }
        }
    }

    // Ensure a repo exists (template/clone paths may already have run `git init`).
    if !repo_path.join(".git").exists() {
        git_cmd(config_path, &["init"]).context("git init failed")?;
    }

    let git_user_name = neo_cli_get(config, profile, "gitUserName").unwrap_or("Neo Bootstrap");
    let git_user_email = neo_cli_get(config, profile, "gitUserEmail").unwrap_or("neo@local");
    let default_branch = neo_cli_get(config, profile, "defaultBranch").unwrap_or("master");
    git_cmd(config_path, &["config", "user.name", git_user_name])?;
    git_cmd(config_path, &["config", "user.email", git_user_email])?;
    git_cmd(
        config_path,
        &["config", "init.defaultBranch", default_branch],
    )?;

    super::generate_hardware::generate_hardware(config_path, config, false, nix_cmd)?;
    super::paste_settings::paste_settings(
        config_path,
        &PathBuf::from("settings.toml"),
        config,
        false,
        nix_cmd,
    )?;

    git_cmd(config_path, &["add", "."])?;

    super::update_inputs::update_inputs(config_path, false, nix_cmd)?;

    // Final git add + conditional commit (matches Bash exactly)
    git_cmd(config_path, &["add", "."])?;

    if has_staged_changes(config_path) {
        git_cmd(config_path, &["commit", "-m", "Update from neo init"])?;
        println!("Committed initial changes");
    } else {
        println!("✓ No changes to commit (everything is up-to-date)");
    }
    println!("Repository ready at {}", config_path);
    Ok(())
}
