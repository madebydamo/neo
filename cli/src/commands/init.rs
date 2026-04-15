use anyhow::{Context, Result};
use git2::Repository;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::DocumentMut;

pub fn init(config_path: &str, config: &DocumentMut, section: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: smart init at {}", config_path);
        let repo_url = config
            .get(section)
            .and_then(|t| t.get("repoUrl"))
            .and_then(|u| u.as_str())
            .filter(|s| !s.is_empty());
        let bootstrap_method = config
            .get(section)
            .and_then(|t| t.get("bootstrapMethod"))
            .and_then(|v| v.as_str())
            .unwrap_or("template");
        let template = config
            .get(section)
            .and_then(|t| t.get("template"))
            .and_then(|v| v.as_str())
            .unwrap_or("github:madebydamo/neo#homeserver");
        let git_user_name = config
            .get(section)
            .and_then(|t| t.get("gitUserName"))
            .and_then(|v| v.as_str())
            .unwrap_or("Neo Bootstrap");
        let git_user_email = config
            .get(section)
            .and_then(|t| t.get("gitUserEmail"))
            .and_then(|v| v.as_str())
            .unwrap_or("neo@local");
        let default_branch = config
            .get(section)
            .and_then(|t| t.get("defaultBranch"))
            .and_then(|v| v.as_str())
            .unwrap_or("master");
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
        let repo_url = config
            .get(section)
            .and_then(|t| t.get("repoUrl"))
            .and_then(|u| u.as_str())
            .filter(|s| !s.is_empty());

        let bootstrap_method = config
            .get(section)
            .and_then(|t| t.get("bootstrapMethod"))
            .and_then(|v| v.as_str())
            .unwrap_or("template");

        if repo_url.is_some() && bootstrap_method == "clone" {
            let _ = Command::new("git")
                .current_dir(config_path)
                .args(["clone", repo_url.unwrap(), "."])
                .status();
        } else {
            let template = config
                .get(section)
                .and_then(|t| t.get("template"))
                .and_then(|v| v.as_str())
                .unwrap_or("github:madebydamo/neo#homeserver");

            let _ = Command::new("nix")
                .current_dir(config_path)
                .args([
                    "--extra-experimental-features",
                    "nix-command flakes",
                    "flake",
                    "init",
                    "-t",
                    template,
                ])
                .status()
                .context("flake init failed")?;

            let _ = Command::new("git")
                .current_dir(config_path)
                .arg("init")
                .status();

            if let Some(url) = repo_url {
                let _ = Command::new("git")
                    .current_dir(config_path)
                    .args(["remote", "add", "origin", url])
                    .status();
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
        config
            .get(section)
            .and_then(|t| t.get("gitUserName"))
            .and_then(|v| v.as_str())
            .unwrap_or("Neo Bootstrap"),
    )?;
    cfg.set_str(
        "user.email",
        config
            .get(section)
            .and_then(|t| t.get("gitUserEmail"))
            .and_then(|v| v.as_str())
            .unwrap_or("neo@local"),
    )?;
    cfg.set_str(
        "init.defaultBranch",
        config
            .get(section)
            .and_then(|t| t.get("defaultBranch"))
            .and_then(|v| v.as_str())
            .unwrap_or("master"),
    )?;

    crate::commands::generate_hardware::generate_hardware(config_path, false)?;
    crate::commands::paste_settings::paste_settings(
        config_path,
        &PathBuf::from("settings.toml"),
        config,
        false,
    )?;
    crate::commands::update_inputs::update_inputs(config_path, false)?;

    // Final git add + conditional commit (matches Bash exactly)
    let _ = Command::new("git")
        .current_dir(config_path)
        .arg("add")
        .arg(".")
        .status();

    if Command::new("git")
        .current_dir(config_path)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        println!("✓ No changes to commit (everything is up-to-date)");
    } else {
        let _ = Command::new("git")
            .current_dir(config_path)
            .args(["commit", "-m", "Update from neo init"])
            .status();
        println!("Committed initial changes");
    }
    println!("Repository ready at {}", config_path);
    Ok(())
}
