use anyhow::{Context, Result};
use git2::Repository;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::DocumentMut;

use crate::commands::profile::neo_cli_get;
use crate::commands::{execute_command, git_cmd, has_staged_changes, run_nix};

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

        if repo_url.is_some() && bootstrap_method == "clone" {
            let desc = format!("git clone {} (in {})", repo_url.unwrap(), config_path);
            execute_command(
                Command::new("git").current_dir(config_path).args([
                    "clone",
                    repo_url.unwrap(),
                    ".",
                ]),
                &desc,
            )?;
        } else {
            let template = neo_cli_get(config, profile, "template")
                .unwrap_or("github:madebydamo/neo#homeserver");

            run_nix(config_path, nix_cmd, &["flake", "init", "-t", template])?;

            execute_command(
                Command::new("git").current_dir(config_path).arg("init"),
                "git init",
            )?;

            if let Some(url) = repo_url {
                let desc = format!("git remote add origin {} (in {})", url, config_path);
                execute_command(
                    Command::new("git")
                        .current_dir(config_path)
                        .args(["remote", "add", "origin", url]),
                    &desc,
                )?;
            }
        }
    }

    let repo = if has_git {
        Repository::open(repo_path)?
    } else {
        Repository::init(repo_path).context("git init failed")?
    };

    let mut cfg = repo.config().context("git config")?;
    cfg.set_str(
        "user.name",
        neo_cli_get(config, profile, "gitUserName").unwrap_or("Neo Bootstrap"),
    )?;
    cfg.set_str(
        "user.email",
        neo_cli_get(config, profile, "gitUserEmail").unwrap_or("neo@local"),
    )?;
    cfg.set_str(
        "init.defaultBranch",
        neo_cli_get(config, profile, "defaultBranch").unwrap_or("master"),
    )?;

    crate::commands::generate_hardware::generate_hardware(config_path, &config, false, nix_cmd)?;
    crate::commands::paste_settings::paste_settings(
        config_path,
        &PathBuf::from("settings.toml"),
        config,
        false,
        nix_cmd,
    )?;

    git_cmd(config_path, &["add", "."])?;

    crate::commands::update_inputs::update_inputs(config_path, false, nix_cmd)?;

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
