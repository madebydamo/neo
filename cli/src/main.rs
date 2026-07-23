use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use std::env;
use std::path::PathBuf;
use std::process::Command;
use toml_edit::DocumentMut;

pub mod commands;
use crate::commands::profile::{resolve_config_path, resolve_profile};
use crate::commands::{
    activate::activate,
    build::build,
    edit::edit,
    execute_command,
    generate_hardware::generate_hardware,
    generation::{generation_boot, generation_help, generation_list, generation_switch},
    git::git,
    init::init,
    migrate::migrate,
    nuke::nuke,
    paste_settings::paste_settings,
    update::update,
    update_inputs::update_inputs,
    web::web,
};
#[derive(Parser)]
#[command(name = "neo", version, about = "Neo Homeserver CLI", long_about = None)]
struct Cli {
    /// Path to settings.toml. If /etc/neo/settings.toml exists it is used as the default
    /// source (esp. for `paste-settings`, which writes merged config to configPath/settings.toml).
    /// Falls back to ./settings.toml (if present) or baked Nix defaults. The TOML (or defaults)
    /// defines configPath per local/server profile under [neo-cli].
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

    /// CLI profile: `local` (laptop / nix run) or `server` (homeserver).
    /// Default: server if /etc/neo/settings.toml exists, else local. Env: NEO_PROFILE.
    #[arg(long, env = "NEO_PROFILE", default_value = "", global = true)]
    profile: String,

    /// Alias for --profile. Also accepts legacy names neo-cli (→ local) and neo-service (→ server).
    /// Env: NEO_SECTION.
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
    Migrate,
    Build,
    Activate {
        #[arg(long, env = "NEO_ACTIVATION_SUFFIX")]
        activation_suffix: Option<String>,
    },
    Update {
        #[arg(long, env = "NEO_UPDATE_SUFFIX")]
        update_suffix: Option<String>,
    },
    Nuke,
    Web,
    Edit,
    Git,
    Lg,
    DockerUpdate {
        container: String,
    },
    /// List / switch / boot NixOS system generations.
    Generation {
        #[command(subcommand)]
        action: Option<GenerationAction>,
    },
}

#[derive(Subcommand, Debug)]
enum GenerationAction {
    /// List system profile generations.
    List,
    /// Switch the running system to generation N.
    Switch { n: u64 },
    /// Set boot default to generation N (next reboot).
    Boot { n: u64 },
}

pub fn load_or_default_settings(path: &PathBuf, _profile: &str) -> Result<DocumentMut> {
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
                        // Nested profile tables (local/server): merge keys, do not replace whole table.
                        if let (Some(bt), Some(ot)) =
                            (b.get_mut(ik).and_then(|x| x.as_table_mut()), iv.as_table())
                        {
                            for (nk, nv) in ot.iter() {
                                bt.insert(nk, nv.clone());
                            }
                        } else {
                            b.insert(ik, iv.clone());
                        }
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

    let profile = resolve_profile(&cli.profile, &cli.section, etc_settings.exists());

    // On a full install, run as homeserver so configPath ownership and git identity match.
    if etc_settings.exists() && env::var("USER").unwrap_or_default() != "homeserver" {
        let sudo_bin = cli.sudo_path.as_deref().unwrap_or("sudo");
        execute_command(
            Command::new(sudo_bin)
                .arg("-u")
                .arg("homeserver")
                .arg(
                    "--preserve-env=NEO_NEO_INPUT,NEO_TEMPLATE,NEO_REMOTE_URL,NIX_BINARY_PATH,SUDO_BINARY_PATH,NEO_ACTIVATION_SUFFIX,NEO_UPDATE_SUFFIX,NEO_SECTION,NEO_PROFILE",
                )
                .args(env::args()),
        )?;
        return Ok(());
    }

    let mut doc = load_or_default_settings(&settings_path, &profile)?;
    // Merge CLI overrides into shared neo-cli
    if let Some(v) = cli.neo_input {
        if let Some(table) = doc.get_mut("neo-cli").and_then(|t| t.as_table_mut()) {
            table.insert("neoInput", toml_edit::value(v));
        }
    }
    if let Some(v) = cli.template {
        if let Some(table) = doc.get_mut("neo-cli").and_then(|t| t.as_table_mut()) {
            table.insert("template", toml_edit::value(v));
        }
    }
    if let Some(v) = cli.remote_url {
        if let Some(table) = doc.get_mut("neo-cli").and_then(|t| t.as_table_mut()) {
            table.insert("repoUrl", toml_edit::value(v));
        }
    }

    let config_path = resolve_config_path(&doc, &profile);

    // The writable settings file lives under configPath (same as `neo edit` uses).
    // The original CLI --settings (or /etc/neo/settings.toml) is only the source we loaded from.
    let web_settings_path: PathBuf = PathBuf::from(format!("{}/settings.toml", config_path));

    let dry_run = cli.dry_run;
    let nix_cmd = cli.nix_path.as_deref().unwrap_or("nix");
    let sudo_cmd = cli.sudo_path.as_deref().unwrap_or("sudo");
    if dry_run {
        println!(
            "=== DRY-RUN ENABLED for {:?} (profile={}) ===",
            command, profile
        );
    }

    match command {
        Commands::GenerateHardware => generate_hardware(&config_path, &doc, dry_run, nix_cmd),
        Commands::PasteSettings => {
            paste_settings(&config_path, &settings_path, &doc, dry_run, nix_cmd)
        }
        Commands::Init => init(&config_path, &doc, &profile, dry_run, nix_cmd),
        Commands::UpdateInputs => update_inputs(&config_path, dry_run, nix_cmd),
        Commands::Update { update_suffix } => update(
            &config_path,
            &doc,
            &profile,
            dry_run,
            nix_cmd,
            update_suffix.as_deref(),
        ),
        Commands::Migrate => migrate(&config_path, &settings_path, dry_run),
        Commands::Build => build(&config_path, &doc, dry_run, nix_cmd),
        Commands::Activate { activation_suffix } => activate(
            &config_path,
            dry_run,
            nix_cmd,
            sudo_cmd,
            activation_suffix.as_deref(),
        ),
        Commands::Nuke => nuke(&config_path, dry_run, nix_cmd),
        Commands::Web => web(&doc, web_settings_path, nix_cmd, &config_path),
        Commands::Edit => edit(&config_path, dry_run),
        Commands::Git => git(&config_path, dry_run),
        Commands::Lg => git(&config_path, dry_run),
        Commands::DockerUpdate { container } => {
            use crate::commands::docker_update::docker_update;
            docker_update(&container)
        }
        Commands::Generation { action } => match action {
            Some(GenerationAction::List) => generation_list(dry_run),
            Some(GenerationAction::Switch { n }) => generation_switch(n, dry_run, sudo_cmd),
            Some(GenerationAction::Boot { n }) => generation_boot(n, dry_run, sudo_cmd),
            None => generation_help(),
        },
    }
}
