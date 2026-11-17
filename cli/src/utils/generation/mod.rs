//! List and switch NixOS system generations (profile under `/nix/var/nix/profiles/system`):
//! the profile's generations ([`profiles`]), which one ran when ([`timeline`]),
//! and the generation recorded in activation commit messages ([`commit`]).

use anyhow::{bail, Result};
use std::path::PathBuf;
use std::process::Command;

use super::command::execute_command;

mod commit;
mod profiles;
mod timeline;

pub use commit::{parse_generation_from_message, record_generation_in_commit};
pub use profiles::{
    current_generation_number, generation_links, list_system_generations,
    running_generation_number, system_profile_available,
};
pub use timeline::GenerationTimeline;

const SYSTEM_PROFILE: &str = "/nix/var/nix/profiles/system";
const PROFILES_DIR: &str = "/nix/var/nix/profiles";
const RUNNING_SYSTEM: &str = "/run/current-system";
/// `<epoch> <system store path>` per activation (boot, switch, test); written by
/// `system.activationScripts.neo-system-history` (nix/modules/core/base.nix).
/// Lives on the root filesystem, so a Neo data restore does not rewind it.
pub const ACTIVATION_LOG: &str = "/var/lib/neo/system-activations";

/// How a system generation is applied (`neo activate --mode`, generation switch).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum GenerationMode {
    /// Live `nixos-rebuild switch` (default; interactive UI/CLI).
    #[default]
    Switch,
    /// `nixos-rebuild boot`: set boot default only; running system unchanged until reboot.
    Boot,
}

impl GenerationMode {
    /// `switch-to-configuration` action (also the CLI subcommand).
    pub fn as_str(self) -> &'static str {
        match self {
            GenerationMode::Switch => "switch",
            GenerationMode::Boot => "boot",
        }
    }
}

/// Switch or set boot default for generation `n`.
/// Uses shell: `nix-env --switch-generation` + generation's `switch-to-configuration`.
///
/// **Must not run inside the neo-web process** for live `switch`: the activation
/// stops `neo-web.service`. Web UI triggers this via `systemd-run` oneshot.
pub fn switch_system_generation(n: u64, mode: GenerationMode, sudo_cmd: &str) -> Result<()> {
    let link = PathBuf::from(format!("/nix/var/nix/profiles/system-{}-link", n));
    if !link.exists() && !link.is_symlink() {
        bail!("generation {} not found ({})", n, link.display());
    }

    // Check before touching the profile: a failure must not leave it moved
    // to a generation whose bootloader entry / activation never ran.
    let stc = link.join("bin/switch-to-configuration");
    if !stc.exists() {
        bail!("switch-to-configuration missing for generation {n} (store path gone?)");
    }

    // Point the system profile at this generation (boot default + current pointer).
    execute_command(Command::new(sudo_cmd).args([
        "-n",
        "nix-env",
        "-p",
        SYSTEM_PROFILE,
        "--switch-generation",
        &n.to_string(),
    ]))?;
    execute_command(
        Command::new(sudo_cmd)
            .arg("-n")
            .arg(&stc)
            .arg(mode.as_str()),
    )
}
