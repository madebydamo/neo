//! External programs neo runs, one place for their paths.
//!
//! Every program takes its `*_BINARY_PATH` env var first; the Nix modules set
//! them for every unit that runs neo (neo-web, neo-security, the updater) and
//! the CLI wrapper sets `SOPS_BINARY_PATH`.
//!
//! Without the variable:
//! - docker, zfs and nix (`system_tool`) use the NixOS system profile when it
//!   has the program: a systemd unit's PATH is a closed list, so a bare name
//!   fails there with ENOENT. Where the profile lacks it (a dev machine, a
//!   non-NixOS host) the PATH lookup stays.
//! - sudo and sops look in PATH: the profile's sudo is not the setuid wrapper
//!   (`/run/wrappers/bin`), and sops is no system package.
//! - The interactive CLI resolves nix from PATH ([`NIX_CLI`], behind
//!   `--nix-path` / `NIX_BINARY_PATH`): a shell, `nix run` and the restore
//!   installer (whose /run/current-system is the installer's) bring their own
//!   nix, and the web passes its nix to the CLI jobs it starts.

use std::path::Path;

use super::command::env_or;

const SYSTEM_PROFILE_BIN: &str = "/run/current-system/sw/bin";

/// nix when the CLI has neither `--nix-path` nor `NIX_BINARY_PATH`.
pub const NIX_CLI: &str = "nix";

/// sudo when the CLI has neither `--sudo-path` nor `SUDO_BINARY_PATH`.
pub const SUDO_CLI: &str = "sudo";

/// `$var`, else the system profile's `name` when present, else `name`.
fn system_tool(var: &str, name: &str) -> String {
    let system = format!("{SYSTEM_PROFILE_BIN}/{name}");
    let fallback = if Path::new(&system).exists() {
        system.as_str()
    } else {
        name
    };
    env_or(var, fallback)
}

pub fn docker() -> String {
    system_tool("DOCKER_BINARY_PATH", "docker")
}

pub fn zfs() -> String {
    system_tool("ZFS_BINARY_PATH", "zfs")
}

/// nix for long-running services (the web UI); the CLI uses [`NIX_CLI`].
pub fn nix() -> String {
    system_tool("NIX_BINARY_PATH", "nix")
}

pub fn sudo() -> String {
    env_or("SUDO_BINARY_PATH", SUDO_CLI)
}

pub fn sops() -> String {
    env_or("SOPS_BINARY_PATH", "sops")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_tool_prefers_env_then_profile_then_path() {
        // Unset and empty variables fall through.
        let var = "NEO_BINS_TEST_UNSET_VARIABLE";
        let missing = system_tool(var, "neo-no-such-program");
        assert_eq!(missing, "neo-no-such-program");
        let sh = system_tool(var, "sh");
        if Path::new("/run/current-system/sw/bin/sh").exists() {
            assert_eq!(sh, "/run/current-system/sw/bin/sh");
        } else {
            assert_eq!(sh, "sh");
        }
        // PATH is set for every test run.
        assert_ne!(system_tool("PATH", "sh"), "sh");
    }
}
