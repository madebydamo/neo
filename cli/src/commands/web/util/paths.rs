use std::path::{Path, PathBuf};

use crate::utils::command::{env_or, sudo_bin};

/// neo-web state directory (version notes, proposals): systemd's `$STATE_DIRECTORY` (neo-web),
/// else `$XDG_STATE_HOME/neo` or `~/.local/state/neo` (local / dev runs).
pub fn state_dir() -> Option<PathBuf> {
    if let Ok(s) = std::env::var("STATE_DIRECTORY") {
        if let Some(first) = s.split(':').find(|p| !p.is_empty()) {
            return Some(PathBuf::from(first));
        }
    }
    if let Ok(x) = std::env::var("XDG_STATE_HOME") {
        if !x.is_empty() {
            return Some(PathBuf::from(x).join("neo"));
        }
    }
    let home = std::env::var("HOME").ok().filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join(".local/state/neo"))
}

/// Contents of the token file named by `$var`, trimmed; `None` when unset, unreadable
/// or empty (the feature behind the token stays off).
pub fn read_token_file(var: &str) -> Option<String> {
    let path = std::env::var(var).ok()?;
    let token = std::fs::read_to_string(path).ok()?;
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

pub fn sudo_cmd() -> String {
    sudo_bin()
}

pub fn nix_bin() -> String {
    env_or("NIX_BINARY_PATH", "/run/current-system/sw/bin/nix")
}

pub fn neo_bin() -> String {
    env_or("NEO_BINARY_PATH", "/run/current-system/sw/bin/neo")
}

/// Handlebars root. Nix sets this; `cargo run` uses `cli/templates`.
pub fn template_dir() -> String {
    env_or("TEMPLATE_DIR", "templates")
}

/// Static asset root. Nix sets this; `cargo run` uses `cli/static`.
pub fn static_dir() -> String {
    env_or("STATIC_DIR", "static")
}

/// Docker CLI for inspect/pull. neo-web's systemd PATH does not include docker,
/// so a bare `"docker"` lookup fails with ENOENT (`os error 2`).
pub fn docker_bin() -> String {
    env_or("DOCKER_BINARY_PATH", "/run/current-system/sw/bin/docker")
}

pub fn zfs_bin() -> String {
    env_or("ZFS_BINARY_PATH", "/run/current-system/sw/bin/zfs")
}

pub fn rsync_bin() -> String {
    env_or("RSYNC_BINARY_PATH", "/run/current-system/sw/bin/rsync")
}

/// systemctl for read-only unit queries (`systemctl show`). neo-web's PATH is a
/// closed list without systemd, so prefer an explicit env path, then the NixOS
/// system profile, then a bare PATH lookup (dev machines).
pub fn systemctl_bin() -> String {
    let system = "/run/current-system/sw/bin/systemctl";
    let fallback = if Path::new(system).exists() {
        system
    } else {
        "systemctl"
    };
    env_or("SYSTEMCTL_BINARY_PATH", fallback)
}

#[cfg(test)]
mod tests {
    use super::{docker_bin, static_dir, template_dir};
    use crate::utils::command::non_empty_or;

    #[test]
    fn docker_bin_default_is_nixos_system_path_not_bare_name() {
        assert!(
            std::env::var("DOCKER_BINARY_PATH")
                .ok()
                .filter(|p| !p.is_empty())
                .is_none(),
            "DOCKER_BINARY_PATH is set; unset it to assert the default"
        );
        assert_eq!(docker_bin(), "/run/current-system/sw/bin/docker");
    }

    #[test]
    fn docker_bin_uses_env_when_set() {
        assert_eq!(
            non_empty_or(
                Some("/nix/store/abc/bin/docker".to_string()),
                "/run/current-system/sw/bin/docker"
            ),
            "/nix/store/abc/bin/docker"
        );
    }

    #[test]
    fn asset_dirs_default_to_repo_relative_paths() {
        assert!(
            std::env::var("TEMPLATE_DIR")
                .ok()
                .filter(|p| !p.is_empty())
                .is_none(),
            "TEMPLATE_DIR is set; unset it to assert the default"
        );
        assert!(
            std::env::var("STATIC_DIR")
                .ok()
                .filter(|p| !p.is_empty())
                .is_none(),
            "STATIC_DIR is set; unset it to assert the default"
        );
        assert_eq!(template_dir(), "templates");
        assert_eq!(static_dir(), "static");
    }

    #[test]
    fn docker_bin_treats_empty_env_as_unset() {
        assert_eq!(
            non_empty_or(Some(String::new()), "/run/current-system/sw/bin/docker"),
            "/run/current-system/sw/bin/docker"
        );
    }
}
