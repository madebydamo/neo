use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use std::env;
use std::path::PathBuf;
use std::process::Command;

pub mod commands;
pub mod utils;

use crate::commands::{
    activate::{activate, ActivationMode},
    build::build,
    docker_update::docker_update,
    edit::edit,
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
use crate::utils::locks::{LockError, LockGuard, LockManager, LockSpec, OpInfo};
use crate::utils::{
    execute_command, load_or_default_settings, resolve_config_path, resolve_profile,
    set_profile_str, OperationKind, OperationLog,
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

    /// Alias for --profile (`local` or `server`). Env: NEO_SECTION.
    #[arg(long, env = "NEO_SECTION", default_value = "", global = true)]
    section: String,

    /// path to nix executable
    #[arg(long, env = "NIX_BINARY_PATH", global = true)]
    nix_path: Option<String>,

    /// path to sudo executable
    #[arg(long, env = "SUDO_BINARY_PATH", global = true)]
    sudo_path: Option<String>,

    /// Seconds to wait for a conflicting operation (activation, update, restore, …)
    /// to finish before giving up. Default 0: fail at once. Env: NEO_LOCK_WAIT.
    #[arg(long, env = "NEO_LOCK_WAIT", value_name = "SECONDS", global = true)]
    lock_wait: Option<u64>,

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
        /// How to apply the new generation: live switch (default) or boot-only (next reboot).
        #[arg(
            long,
            env = "NEO_ACTIVATION_MODE",
            value_enum,
            default_value_t = ActivationMode::Switch
        )]
        mode: ActivationMode,
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
    /// Reinstall a machine from its config repository (run as root on a live
    /// system with the target mounted): restores the escrowed host key, then
    /// nixos-install.
    Restore {
        /// Config repository URL (git clone).
        repo: String,
        /// Root of the mounted target system.
        #[arg(long, default_value = "/mnt")]
        target: PathBuf,
        /// Admin ssh-ed25519 private key (default: sops uses ~/.ssh/id_ed25519).
        #[arg(long)]
        key: Option<PathBuf>,
        /// nixosConfigurations entry to install.
        #[arg(long, default_value = "neo")]
        config: String,
        /// Only clone the repo and restore the host key.
        #[arg(long)]
        no_install: bool,
    },
    /// Secret settings kept outside settings.toml.
    Secrets {
        #[command(subcommand)]
        action: SecretsAction,
    },
}

#[derive(Subcommand, Debug)]
enum SecretsAction {
    /// Move secret settings from settings.toml into secrets.yaml (unencrypted).
    Split,
    /// Encrypt the secrets for this machine and core.ssh.authorizedKeys (sops).
    Enable,
    /// Decrypt secrets.sops.yaml back into secrets.yaml.
    Disable,
    /// Re-encrypt for the current host and authorized keys.
    Rekey,
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

fn main() {
    let cli = Cli::parse();
    // Rust ignores SIGPIPE, so `println!` panics once a pipe reader such as
    // `neo generation list | head` exits. One-shot commands get the default
    // back and end quietly; `neo web` keeps it ignored so a closed pipe to a
    // child process is an error instead of killing the server.
    if !matches!(cli.command, Some(Commands::Web)) {
        // SAFETY: single-threaded here, before any other thread or handler exists.
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
    }
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
    let settings_path = if let Some(s) = cli.settings.filter(|s| s.exists()) {
        s
    } else if etc_settings.exists() {
        etc_settings.clone()
    } else {
        PathBuf::from("settings.toml")
    };

    let profile = resolve_profile(&cli.profile, &cli.section, etc_settings.exists());

    // On a full install, run as homeserver so configPath ownership and git identity match.
    // `restore` writes into a mounted target (host key, repo) and needs root.
    let needs_root = matches!(command, Commands::Restore { .. });
    if etc_settings.exists() && env::var("USER").unwrap_or_default() != "homeserver" && !needs_root
    {
        let sudo_bin = cli.sudo_path.as_deref().unwrap_or("sudo");
        // The child prints its own error. A failed `neo init` still exits
        // through this sudo, so do not describe that as a user-switch failure.
        execute_command(
            Command::new(sudo_bin)
                .arg("-u")
                .arg("homeserver")
                .arg(
                    "--preserve-env=NEO_NEO_INPUT,NEO_TEMPLATE,NEO_REMOTE_URL,NIX_BINARY_PATH,SUDO_BINARY_PATH,NEO_ACTIVATION_SUFFIX,NEO_ACTIVATION_MODE,NEO_UPDATE_SUFFIX,NEO_GENSWITCH_SUFFIX,NEO_LOCK_WAIT,NEO_SECTION,NEO_PROFILE,TEMPLATE_DIR,STATIC_DIR,DEFAULT_SETTINGS_PATH",
                )
                .args(env::args()),
        )?;
        return Ok(());
    }

    let mut doc = load_or_default_settings(&settings_path)?;
    // Overrides apply to the active profile so a server command does not
    // inherit a laptop path written on the shared table.
    if let Some(v) = cli.neo_input {
        set_profile_str(&mut doc, &profile, "neoInput", &v);
    }
    if let Some(v) = cli.template {
        set_profile_str(&mut doc, &profile, "template", &v);
    }
    if let Some(v) = cli.remote_url {
        set_profile_str(&mut doc, &profile, "repoUrl", &v);
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

    // Held until the command returns; nested `neo` calls inherit it.
    // neo-bootstrap may start mid-`nixos-rebuild switch` while activate still
    // holds the system lock. Re-init is unnecessary when the repo is already
    // there — skip instead of failing the oneshot (and neo-web Requires=).
    let _lock = if dry_run {
        None
    } else {
        match try_acquire_command_lock(&command, cli.lock_wait, &config_path) {
            Ok(g) => g,
            Err(LockAcquireError::SkipInit) => {
                println!(
                    "✓ Config repository already present; system operation in progress — skipping init"
                );
                return Ok(());
            }
            Err(LockAcquireError::Other(e)) => return Err(e),
        }
    };

    match command {
        Commands::GenerateHardware => generate_hardware(&config_path, &doc, dry_run),
        Commands::PasteSettings => paste_settings(&config_path, &settings_path, &doc, dry_run),
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
        Commands::Activate {
            activation_suffix,
            mode,
        } => activate(
            &config_path,
            dry_run,
            nix_cmd,
            sudo_cmd,
            activation_suffix.as_deref(),
            mode,
        ),
        Commands::Nuke => nuke(&config_path, dry_run),
        Commands::Web => web(web_settings_path, nix_cmd, &config_path),
        Commands::Edit => edit(&config_path, dry_run),
        Commands::Git | Commands::Lg => git(&config_path, dry_run),
        Commands::DockerUpdate { container } => docker_update(&container),
        Commands::Generation { action } => match action {
            Some(GenerationAction::List) => generation_list(dry_run, sudo_cmd),
            Some(GenerationAction::Switch { n }) => generation_switch(n, dry_run, sudo_cmd),
            Some(GenerationAction::Boot { n }) => generation_boot(n, dry_run, sudo_cmd),
            None => generation_help(),
        },
        Commands::Restore {
            repo,
            target,
            key,
            config,
            no_install,
        } => commands::restore::restore(
            &commands::restore::RestoreArgs {
                repo,
                target,
                key,
                config,
                no_install,
            },
            dry_run,
        ),
        Commands::Secrets { action } => match action {
            SecretsAction::Split => commands::secrets::split(&config_path, dry_run, nix_cmd),
            SecretsAction::Enable => commands::secrets::enable(&config_path, dry_run, nix_cmd),
            SecretsAction::Disable => commands::secrets::disable(&config_path, dry_run),
            SecretsAction::Rekey => commands::secrets::rekey(&config_path, dry_run),
        },
    }
}

/// Operation lock a subcommand needs, and the web-tracked op it runs as (if any).
/// See `utils::locks` for the scope model.
fn command_lock(command: &Commands) -> Option<(LockSpec, OpInfo, Option<OperationLog>)> {
    let system =
        |kind: &str, label: &str| Some((LockSpec::system_change(), OpInfo::new(kind, label), None));
    // Set when the web UI started this command (its monitor must see a lock failure).
    let web_op = |kind: OperationKind, suffix: Option<String>| {
        suffix
            .filter(|s| !s.is_empty())
            .map(|s| OperationLog::new(kind, &s))
    };
    let with_op = |mut t: (LockSpec, OpInfo, Option<OperationLog>), op: Option<OperationLog>| {
        if let Some(op) = &op {
            t.1 = t.1.with_op_id(op.id());
        }
        t.2 = op;
        Some(t)
    };
    match command {
        Commands::Activate {
            activation_suffix,
            mode: _,
        } => with_op(
            system("activation", "Activation")?,
            web_op(OperationKind::Activation, activation_suffix.clone()),
        ),
        Commands::Update { update_suffix } => with_op(
            system("update", "Update")?,
            web_op(OperationKind::Update, update_suffix.clone()),
        ),
        Commands::Generation {
            action: Some(GenerationAction::Switch { .. } | GenerationAction::Boot { .. }),
        } => with_op(
            system("generation", "Generation switch")?,
            web_op(
                OperationKind::Generation,
                env::var("NEO_GENSWITCH_SUFFIX").ok(),
            ),
        ),
        Commands::UpdateInputs => system("update", "Flake input update"),
        Commands::Init => system("init", "Config init"),
        Commands::Migrate => system("migrate", "Config migration"),
        Commands::Build => system("build", "Build"),
        Commands::PasteSettings => system("paste-settings", "Settings paste"),
        Commands::GenerateHardware => system("generate-hardware", "Hardware config generation"),
        Commands::Nuke => system("nuke", "Config removal"),
        Commands::Secrets { .. } => system("secrets", "Secret settings change"),
        Commands::DockerUpdate { container } => {
            let unit = format!(
                "docker-{}",
                container.strip_prefix("docker-").unwrap_or(container)
            );
            Some((
                LockSpec::unit(&unit),
                OpInfo::new("pull", format!("Image update of {unit}")),
                None,
            ))
        }
        // Long-lived / interactive / read-only.
        Commands::Web
        | Commands::Edit
        | Commands::Git
        | Commands::Lg
        | Commands::Generation { .. }
        | Commands::Restore { .. } => None,
    }
}

enum LockAcquireError {
    /// `neo init` hit a system switch lock and the config repo is already ready.
    SkipInit,
    Other(anyhow::Error),
}

/// Take the command's lock (waiting up to `wait_secs`). Exclusive scopes are
/// exported to child processes so nested `neo` calls do not block on them.
fn try_acquire_command_lock(
    command: &Commands,
    wait_secs: Option<u64>,
    config_path: &str,
) -> std::result::Result<Option<LockGuard>, LockAcquireError> {
    let Some((spec, info, web_op)) = command_lock(command) else {
        return Ok(None);
    };
    let wait = std::time::Duration::from_secs(wait_secs.unwrap_or(0));
    match LockManager::system().acquire(&spec, &info, wait) {
        Ok(guard) => {
            guard.export_to_children();
            Ok(Some(guard))
        }
        Err(e) => {
            let msg = e.to_string();
            if let Some(op) = web_op {
                op.write_state("failed", "locked", Some(&msg), None);
            }
            if matches!(command, Commands::Init)
                && init_skippable_while_system_locked(&e, config_path)
            {
                return Err(LockAcquireError::SkipInit);
            }
            Err(LockAcquireError::Other(anyhow::anyhow!(msg)))
        }
    }
}

/// True when a concurrent activate/update/generation holds `system` and the
/// server config repo already looks initialized (git + flake). Used so
/// neo-bootstrap's `neo init` does not fail the switch under Activation.
fn init_skippable_while_system_locked(err: &LockError, config_path: &str) -> bool {
    let LockError::Conflict(c) = err else {
        return false;
    };
    let switch_kind = |k: &str| matches!(k, "activation" | "update" | "generation");
    let holders = if c.holders.is_empty() {
        LockManager::system().holders()
    } else {
        c.holders.clone()
    };
    let blocked_by_switch = if holders.is_empty() {
        // Holder file gone already; match the conflict message label.
        let m = c.to_string();
        m.contains("Activation in progress")
            || m.contains("Update in progress")
            || m.contains("Generation switch in progress")
            || m.contains("Flake input update in progress")
    } else {
        holders.iter().any(|h| switch_kind(&h.kind))
    };
    if !blocked_by_switch {
        return false;
    }
    let p = std::path::Path::new(config_path);
    p.join(".git").is_dir() && p.join("flake.nix").is_file()
}
