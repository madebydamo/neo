use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use toml_edit::DocumentMut;

use crate::utils::{git_cmd, has_staged_changes, neo_cli_get, resolve_template, template};

pub fn init(
    config_path: &str,
    config: &DocumentMut,
    profile: &str,
    dry_run: bool,
    nix_cmd: &str,
) -> Result<()> {
    let repo_url = neo_cli_get(config, profile, "repoUrl").filter(|s| !s.is_empty());
    let bootstrap_method = neo_cli_get(config, profile, "bootstrapMethod").unwrap_or("template");
    let template = resolve_template(config, profile);
    // Machine git identity (core.git); baked defaults always carry both keys.
    let git_user_name = core_git_get(config, "userName").unwrap_or("Neo");
    let git_user_email = core_git_get(config, "userEmail").unwrap_or("neo@local");
    let default_branch = neo_cli_get(config, profile, "defaultBranch").unwrap_or("master");
    if dry_run {
        println!(
            "DRY-RUN: smart init at {} (profile={})",
            config_path, profile
        );
        println!("  bootstrapMethod: {}", bootstrap_method);
        if let Some(url) = repo_url {
            println!("  repoUrl: {}", url);
        }
        println!("  template: {}", template);
        println!("  core.git.userName: {}", git_user_name);
        println!("  core.git.userEmail: {}", git_user_email);
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
        if let (Some(url), "clone") = (repo_url, bootstrap_method) {
            git_cmd(config_path, &["clone", url, "."])?;
        } else {
            template::install(config_path, &template, nix_cmd)?;
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

    git_cmd(config_path, &["config", "user.name", git_user_name])?;
    git_cmd(config_path, &["config", "user.email", git_user_email])?;
    git_cmd(
        config_path,
        &["config", "init.defaultBranch", default_branch],
    )?;

    super::generate_hardware::generate_hardware(config_path, config, false)?;
    super::paste_settings::paste_settings(config_path, Path::new("settings.toml"), config, false)?;

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

fn core_git_get<'a>(config: &'a DocumentMut, key: &str) -> Option<&'a str> {
    config
        .get("core")?
        .get("git")?
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}
