# Neo CLI (optional)

**Most people never need this.** Day-to-day Neo is managed in the **web UI**: change settings, activate, done.

The `neo` command-line tool is for **install bootstrap**, automation, and **power users** who prefer a terminal. Product overview: [../README.md](../README.md). Install paths that mention the CLI: [INSTALL.md](INSTALL.md).

```bash
nix run github:madebydamo/neo#neo -- --help
# on a fully installed Neo host:
neo --help
```

## Global flags

| Flag / env | Purpose |
|------------|---------|
| `--settings FILE` | Path to settings. Default: `/etc/neo/settings.toml` if present, else `./settings.toml`. |
| `--profile local \| server` | Which path profile to use. Default: `server` if `/etc/neo/settings.toml` exists, else `local`. Env: `NEO_PROFILE`. |
| `--section …` | Alias for `--profile`. Legacy: `neo-cli` → local, `neo-service` → server. Env: `NEO_SECTION`. |
| `--dry-run` | Print actions without applying. |
| `--neo-input` / `NEO_NEO_INPUT` | Override Neo input URL. |
| `--template` / `NEO_TEMPLATE` | Override template. |
| `--remote-url` / `NEO_REMOTE_URL` | Override config repo URL. |
| `--nix-path` / `NIX_BINARY_PATH` | Nix binary. |
| `--sudo-path` / `SUDO_BINARY_PATH` | Sudo binary. |

On a full install (`/etc/neo/settings.toml` present), commands re-exec as the `homeserver` user when needed and default to the **server** profile (`neo-cli.server.configPath`). Laptop / `nix run` defaults to the **local** profile (`neo-cli.local.configPath`, default `./build`). Shared keys (template, neoInput, git identity, …) live under `[neo-cli]`.

## Commands (summary)

| Command | Role |
|---------|------|
| `neo init` | Create config from template (or clone); hardware + settings bootstrap |
| `neo web` | Start the web UI (also how laptop-side Path B config editing works) |
| `neo activate` | Build and switch **this** machine to the current config |
| `neo build` | Build without switching |
| `neo update` | Refresh inputs; usually follow with activate |
| `neo update-inputs` | Lower-level input refresh |
| `neo generate-hardware` | Write `hardware-configuration.nix` (`--no-filesystems` if Disko on) |
| `neo paste-settings` | Merge settings into the config tree |
| `neo nuke` | Destroy config at configPath (destructive; prefer `--dry-run` first) |
| `neo edit` | Open settings in `$EDITOR` |
| `neo git` / `neo lg` | Git helpers for the config repo |
| `neo migrate` | Older layout migrations |
| `neo docker-update <name>` | Container image update helper |

**Do not** `activate` a remote machine’s config from your laptop—use [nixos-anywhere](INSTALL.md#path-b--install-from-your-laptop-nixos-anywhere) for first install, then activate **on** the server (or use Activate in the web UI there).

## Typical power-user flows

```bash
neo web                 # UI
neo update && neo activate
neo --dry-run activate
```

Developers building Neo itself: [AGENTS.md](../AGENTS.md).
