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
| `--settings FILE` | Path to settings. Default: `/etc/neo/settings.toml` if present, else `./settings.toml`. A path that does not exist is ignored. |
| `--profile local \| server` | Which path profile to use. Default: `server` if `/etc/neo/settings.toml` exists, else `local`. Env: `NEO_PROFILE`. |
| `--section …` | Alias for `--profile` (`local` or `server`). Env: `NEO_SECTION`. |
| `--dry-run` | Print actions without applying. |
| `--neo-input` / `NEO_NEO_INPUT` | Override this profile's Neo input URL. |
| `--template` / `NEO_TEMPLATE` | Override this profile's template. |
| `--remote-url` / `NEO_REMOTE_URL` | Override the config repo URL (`repoUrl`). |
| `--nix-path` / `NIX_BINARY_PATH` | Nix binary. |
| `--sudo-path` / `SUDO_BINARY_PATH` | Sudo binary. |
| `--lock-wait SECONDS` / `NEO_LOCK_WAIT` | Wait for a conflicting operation instead of failing at once (default 0). |

## Operation locks

Commands that change the system take Neo's **system lock** exclusively: `activate`, `update`, `update-inputs`, `generation switch|boot`, `init`, `build`, `migrate`, `paste-settings`, `generate-hardware`, `nuke`. Only one of them runs at a time, and never while the web UI restores a snapshot, clears app data, repairs the Nix store, or restarts the machine. `docker-update <c>` locks only that container's unit. `web`, `edit`, `git` / `lg` and `generation list` take no lock. A conflict fails right away with the holder, e.g. `Error: Blocked: Activation in progress (started 12:03)`; pass `--lock-wait 600` to wait instead. The scheduled system updater waits up to an hour. Locks are `flock`s under `/run/neo/locks` and are released when the process exits, even after a crash.

## Profiles and users

On a full install (`/etc/neo/settings.toml` present), commands re-exec as the `homeserver` user when needed and default to the **server** profile (`neo-cli.server.configPath`, `neo-cli.server.neoInput`). Laptop / `nix run` defaults to the **local** profile (`neo-cli.local.configPath`, default `./build`). `neoInput` and `template` are per profile. The server profile defaults to `github:madebydamo/neo` (`#homeserver` for the template). A laptop checkout (`git+file:` or a directory template) belongs under `[neo-cli.local]`. Shared keys (`repoUrl`, `bootstrapMethod`, `defaultBranch`) stay under `[neo-cli]`. The git identity for config commits is `core.git.userName` / `core.git.userEmail`.

SSH to a finished host as **`homeserver@…`** for `neo`. **`admin@…`** uses the same keys; running `neo` there re-execs through `sudo -u homeserver`, which needs `core.hashedLinuxPassword`. There is no default password. See [INSTALL.md — first login](INSTALL.md#first-login). `nix run github:madebydamo/neo#neo` is currently **`x86_64-linux` only** (Mac / other laptops: [INSTALL.md](INSTALL.md#when-the-laptop-cannot-build-mac-live-usb-or-wrong-architecture)).

## Commands (summary)

| Command | Role |
|---------|------|
| `neo init` | Create the config repo at `configPath` from the template (or clone `repoUrl` when `bootstrapMethod = "clone"`), write hardware config and settings, commit |
| `neo web` | Start the web UI (also how laptop-side Path B config editing works) |
| `neo activate` | Build and apply **this** machine's config. `--mode switch` (default) switches live; `--mode boot` (or `NEO_ACTIVATION_MODE=boot`) only sets the boot default |
| `neo build` | Build the system and the test VM without switching; commit changes |
| `neo update` | Refresh the template modules, regenerate `flake.nix`, run `nix flake update`, then migrations. Follow with `activate` |
| `neo update-inputs` | Regenerate `flake.nix` from settings (plugins, inputs) without updating the lock file |
| `neo generation list` | List system generations; the running one is marked `(current)` |
| `neo generation switch N` | Switch the running system to generation N |
| `neo generation boot N` | Make generation N the boot default (live after the next reboot) |
| `neo generate-hardware` | Write `hardware-configuration.nix` (adds `--no-filesystems` by itself when Disko is on) |
| `neo paste-settings` | Copy the loaded settings file into `configPath/settings.toml` |
| `neo nuke` | Delete everything except dotfiles in `configPath` (destructive; try `--dry-run` first) |
| `neo edit` | Open `configPath/settings.toml` in `vim` |
| `neo git` / `neo lg` | Open `lazygit` in the config repo |
| `neo migrate` | Apply settings migrations from older layouts (`update` runs it for you) |
| `neo docker-update <container>` | Pull the container's current image again and restart its `docker-<container>` unit |

**Do not** `activate` a remote machine's config from your laptop. Use [nixos-anywhere](INSTALL.md#path-b--install-from-your-laptop-nixos-anywhere) for the first install, then activate **on** the server (or use Activate in the web UI there).

## Typical power-user flows

```bash
neo web                    # UI
neo update && neo activate
neo --dry-run activate
neo activate --mode boot   # build + boot default only; reboot later (manual)
neo generation list
neo generation boot 41     # boot into generation 41 after the next reboot
```

The web UI does the same under **Versioning**: switch to a generation, or **Boot into generation N** (sets the boot default and restarts). **Settings → Power** restarts or shuts down the machine.

Scheduled system-updater uses `services.system-updater.activationMode` (`switch` or `boot`). With `activationMode = "boot"`, `rebootAfterBoot` (default true) schedules a delayed host restart after a successful run that advanced the system profile, so the new generation becomes live; set it false to leave the reboot to you. Interactive `neo activate` and web Activate stay on live switch unless you pass `--mode` / `NEO_ACTIVATION_MODE`, and never restart the host because of this option.

Developers building Neo itself: [AGENTS.md](../AGENTS.md).
