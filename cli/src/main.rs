use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use std::env;
use std::path::PathBuf;
use std::process::Command;
use toml_edit::DocumentMut;

pub mod commands;
use crate::commands::{
    activate::activate, build::build, edit::edit, execute_command, generate_hardware::generate_hardware,
    git::git, init::init, nuke::nuke, paste_settings::paste_settings, update::update,
    update_inputs::update_inputs, web::web,
};
#[derive(Parser)]
#[command(name = "neo", version, about = "Neo Homeserver CLI", long_about = None)]
struct Cli {
    /// Path to settings.toml. If /etc/neo/settings.toml exists it is used as the default
    /// source (esp. for `paste-settings`, which writes merged config to configPath/settings.toml).
    /// Falls back to ./settings.toml (if present) or baked Nix defaults. The TOML (or defaults)
    /// defines configPath per section.
    #[arg(long, value_name = "FILE", global = true)]
    settings: Option<PathBuf>,

    /// Enable dry-run mode: print actions without making changes or running commands (for safety/validation).
    #[arg(long, default_value_t = false, global = true)]
    dry_run: bool,

    /// Override neoInput in the loaded settings.toml (updates TOML for init/paste).
    #[arg(long, env = "NEO_NEO_INPUT", global = true)]
    neo_input: Option<String>,

    /// Override template in the loaded settings.toml.
    #[arg(long, env = "NEO_TEMPLATE", global = true)]
    template: Option<String>,

    /// Override remote URL (repoUrl) in the loaded settings.toml.
    #[arg(long, env = "NEO_REMOTE_URL", global = true)]
    remote_url: Option<String>,

    /// Which settings section to use as base defaults ("cli" or "nixos").
    /// If omitted, defaults to "nixos" if /etc/neo/settings.toml available, else "cli".
    /// Program arg or NEO_SECTION= still overrides (e.g. to force cli on full install).
    #[arg(long, env = "NEO_SECTION", default_value = "", global = true)]
    section: String,

    /// path to nix executable
    #[arg(long, env = "NIX_BINARY_PATH", global = true)]
    nix_path: Option<String>,

    /// path to sudo executable
    #[arg(long, env = "SUDO_BINARY_PATH", global = true)]
    sudo_path: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    GenerateHardware,
    PasteSettings,
    Init,
    UpdateInputs,
    Update,
    Build,
    Activate,
    Nuke,
    Web,
    Edit,
    Git,
    Lg,
}

pub fn load_or_default_settings(path: &PathBuf, _section: &str) -> Result<DocumentMut> {
    let default_str = option_env!("DEFAULT_SETTINGS_TOML").unwrap_or("");
    let mut doc = if !default_str.is_empty() {
        default_str.parse().context("parse default TOML")?
    } else {
        DocumentMut::new()
    };
    if path.exists() {
        let user_str = std::fs::read_to_string(path).context("read user settings.toml")?;
        let user_doc: DocumentMut = user_str.parse().context("parse user TOML")?;
        merge_into(&mut doc, &user_doc);
    }
    Ok(doc)
}

fn merge_into(base: &mut DocumentMut, overlay: &DocumentMut) {
    for (k, v) in overlay.iter() {
        match v {
            toml_edit::Item::Table(t) => {
                if let Some(b) = base.get_mut(k).and_then(|x| x.as_table_mut()) {
                    for (ik, iv) in t.iter() {
                        b.insert(ik, iv.clone());
                    }
                } else {
                    base.insert(k, toml_edit::Item::Table(t.clone()));
                }
            }
            _ => {
                base.insert(k, v.clone());
            }
        }
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let command = match cli.command {
        Some(c) => c,
        None => {
            let _ = Cli::command().print_help();
            return Ok(());
        }
    };

    let etc_settings = PathBuf::from("/etc/neo/settings.toml");
    let settings_path = if cli.settings.clone().map_or(false, |s| s.exists()) {
        cli.settings.unwrap().clone()
    } else if etc_settings.exists() {
        etc_settings.clone()
    } else {
        PathBuf::from("settings.toml")
    };

    let section = if !cli.section.is_empty() {
        cli.section.clone()
    } else if etc_settings.exists() {
        "nixos".to_string()
    } else {
        "cli".to_string()
    };

    if section == "nixos" && env::var("USER").unwrap_or_default() != "homeserver" {
        let sudo_bin = cli.sudo_path.as_deref().unwrap_or("sudo");
        execute_command(Command::new(sudo_bin).arg("-u")
            .arg("homeserver")
            .arg(
                "--preserve-env=NEO_NEO_INPUT,NEO_TEMPLATE,NEO_REMOTE_URL,NIX_BINARY_PATH,SUDO_BINARY_PATH",
            )
            .args(env::args()), "sudo -u homeserver")?;
        return Ok(());
    }

    let mut doc = load_or_default_settings(&settings_path, &section)?;
    // Merge CLI overrides (safe)
    if let Some(v) = cli.neo_input {
        if let Some(table) = doc.get_mut("nixos").and_then(|t| t.as_table_mut()) {
            table.insert("neoInput", toml_edit::value(v.clone()));
        }
        if let Some(table) = doc.get_mut("cli").and_then(|t| t.as_table_mut()) {
            table.insert("neoInput", toml_edit::value(v.clone()));
        }
    }
    if let Some(v) = cli.template {
        if let Some(table) = doc.get_mut("nixos").and_then(|t| t.as_table_mut()) {
            table.insert("template", toml_edit::value(v.clone()));
        }
        if let Some(table) = doc.get_mut("cli").and_then(|t| t.as_table_mut()) {
            table.insert("template", toml_edit::value(v.clone()));
        }
    }
    if let Some(v) = cli.remote_url {
        if let Some(table) = doc.get_mut("nixos").and_then(|t| t.as_table_mut()) {
            table.insert("repoUrl", toml_edit::value(v.clone()));
        }
        if let Some(table) = doc.get_mut("cli").and_then(|t| t.as_table_mut()) {
            table.insert("repoUrl", toml_edit::value(v.clone()));
        }
    }

    let config_path = doc
        .get(&section)
        .and_then(|t| t.get("configPath"))
        .and_then(|v| v.as_str())
        .unwrap_or("./build")
        .to_string();

    let dry_run = cli.dry_run;
    let nix_cmd = cli.nix_path.as_deref().unwrap_or("nix");
    let sudo_cmd = cli.sudo_path.as_deref().unwrap_or("sudo");
    if dry_run {
        println!("=== DRY-RUN ENABLED for {:?} ===", command);
    }

    match command {
        Commands::GenerateHardware => generate_hardware(&config_path, dry_run, nix_cmd),
        Commands::PasteSettings => {
            paste_settings(&config_path, &settings_path, &doc, dry_run, nix_cmd)
        }
        Commands::Init => init(&config_path, &doc, &section, dry_run, nix_cmd),
        Commands::UpdateInputs => update_inputs(&config_path, dry_run, nix_cmd),
        Commands::Update => update(&config_path, dry_run, nix_cmd),
        Commands::Build => build(&config_path, &doc, dry_run, nix_cmd),
        Commands::Activate => activate(&config_path, dry_run, nix_cmd, sudo_cmd),
        Commands::Nuke => nuke(&config_path, dry_run, nix_cmd),
        Commands::Web => web(&doc, nix_cmd, &section),
        Commands::Edit => edit(&config_path, dry_run),
        Commands::Git => git(&config_path, dry_run),
        Commands::Lg => git(&config_path, dry_run),
    }
}