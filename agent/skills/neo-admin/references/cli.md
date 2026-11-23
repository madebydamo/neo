# `neo` CLI reference

Checked against `neo --help` of the current source (each subcommand's `--help`). A server on an older Neo can have fewer commands or flags: `ssh homeserver@HOST 'neo <command> --help'` is the truth for that server.

Every command takes `-h` (short help) and `--help` (long help, with the environment variables).

## Where a command runs

| Where | Commands |
|-------|----------|
| On the server, as `homeserver` | `activate`, `update`, `update-inputs`, `migrate`, `build`, `generation …`, `settings …`, `docker-update`, `secrets …`, `backup rotate-key`, `security …`, `web` (already runs as `neo-web`) |
| On your computer | `bootstrap`, `restore --host`, `backup extract-config`, `backup list --from-store`, and `init` / `build` / `web` for a local config (`--profile local`, default `./build`) |
| On a NixOS installer, as root | `restore` without `--host` |
| Interactive, needs a terminal | `edit` (vim), `git` / `lg` (lazygit), `bootstrap` without `--defaults` |

Without Neo installed: `nix run github:madebydamo/neo#neo -- <command>` (x86_64-linux only).

## Global flags

They work before or after the subcommand (`neo --dry-run activate` = `neo activate --dry-run`).

| Flag | Env | Meaning |
|------|-----|---------|
| `--dry-run` | | Print the actions; change nothing |
| `--settings <FILE>` | | Settings file to load. Default: `/etc/neo/settings.toml` when it exists, else `./settings.toml`, else built-in defaults. It also defines `configPath` per profile under `[neo-cli]` |
| `--profile local\|server` | `NEO_PROFILE` | Default: `server` when `/etc/neo/settings.toml` exists, else `local` |
| `--section local\|server` | `NEO_SECTION` | Alias for `--profile` |
| `--config-path <DIR>` | `NEO_CONFIG_PATH` | Work on this config repo instead of the profile's `configPath` (newer Neo only) |
| `--neo-input <REF>` | `NEO_NEO_INPUT` | Override the profile's `neoInput` |
| `--template <REF>` | `NEO_TEMPLATE` | Override the profile's `template` |
| `--remote-url <URL>` | `NEO_REMOTE_URL` | Override `repoUrl` |
| `--nix-path <PATH>` | `NIX_BINARY_PATH` | nix binary |
| `--sudo-path <PATH>` | `SUDO_BINARY_PATH` | sudo binary |
| `--lock-wait <SECONDS>` | `NEO_LOCK_WAIT` | Wait for a conflicting operation instead of failing at once (default 0) |

Profiles: `[neo-cli.server]` (`configPath`, default `/var/neo/DATA/AppData/configuration`; `neoInput`; `template`) and `[neo-cli.local]` (`configPath` default `./build`). Shared keys stay under `[neo-cli]` (`repoUrl`, `defaultBranch`). On a server, `neo` started as another user re-runs itself as `homeserver` with `sudo -u homeserver`.

## Locks

These take the **system lock**, so only one runs at a time and never during a web UI restore, Clear appdata, Nix store repair or restart: `activate`, `update`, `update-inputs`, `generation switch|boot`, `init`, `build`, `migrate`, `paste-settings`, `generate-hardware`, `nuke`, `secrets …`. `settings patch` takes it shared (like a web UI save). `docker-update <c>` locks only that container's unit. No lock: `web`, `edit`, `git`, `lg`, `generation list`, `restore`, `bootstrap`, `backup …`, `settings get`.

A conflict fails at once: `Error: Blocked: Activation in progress (started 12:03)`. `--lock-wait 600` waits instead. Locks are `flock`s in `/run/neo/locks` and end with the process, also after a crash.

## Apply and update

### `neo activate [--mode switch|boot] [--activation-suffix S]`

Build this machine's config and apply it: `write-flake`, build the system, commit the config repo on the branch `activation_<S>`, then `nixos-rebuild switch` (or `boot`).

- `--mode switch` (default) switches live. `--mode boot` only sets the boot default; the new system runs after the next restart. Env `NEO_ACTIVATION_MODE`.
- `--activation-suffix S` names the operation: log and state in `/tmp/neo-activations/activation_<S>.{log,json}`, branch `activation_<S>`. Default: the current time. Env `NEO_ACTIVATION_SUFFIX`.
- Exit code 4 of `nixos-rebuild` (success with warnings) counts as success. On another failure the branch stays for inspection.
- It prints the reasons when the new system needs a restart (`neo-restart-required`). It never restarts the server itself.
- `neo --dry-run activate` prints the plan only.

### `neo update [--update-suffix S]`

Refresh the template files in the config repo, regenerate `flake.nix`, run `nix flake update`, then `neo migrate` from the new Neo. It does not apply: run `neo activate` next. Log `/tmp/neo-activations/update_<S>.log`.

Template files: a file you did not change is replaced (or removed); a file you changed is merged three-way; on a conflict your version stays and the template's lands next to it as `<file>.neo-new`. Other files (your own `modules/*.nix`) are left alone.

### `neo update-inputs`

Regenerate `flake.nix` from the settings (plugins, inputs) without updating `flake.lock`. `activate` and `update` do this step themselves, so you rarely need it alone.

### `neo build`

Build the system and the test VM without switching, then commit the config repo (`Build: <time>`). For a local config; on a server use `neo --dry-run activate` to look and `activate` to apply.

### `neo migrate`

Apply settings migrations from older layouts to `configPath/settings.toml`. `neo update` runs it for you.

## Generations

| Command | Does |
|---------|------|
| `neo generation list` | Lists the system generations, newest first; `(current)` marks the running one |
| `neo generation switch N` | Switches the running system to generation N now |
| `neo generation boot N` | Makes generation N the boot default (runs after the next restart) |

Data and the config repo are not touched. The next activation or nightly update builds the repo's settings again. The web UI does the same under **Versioning**.

## Settings

### `neo settings get <PATH>`

Print one value of `configPath/settings.toml` as JSON; `null` when unset. `PATH` is dotted: `services.swag.domain`, `core.hostname`, `services.immich` (a whole table), `neo-cli`. Values kept in a secrets file are not shown; a secret still inline in `settings.toml` is.

### `neo settings patch` (JSON on stdin)

Apply changes like a web UI save. Secret values go to the secrets file when the config keeps one.

```bash
echo '[
  {"set":   {"path": ["services","searxng","enabled"], "value": true}},
  {"unset": {"path": ["services","pihole","localIP"], "secret": false}}
]' | neo settings patch          # prints: Saved 2 change(s) to …/settings.toml
```

- `set`: `path` (list of keys) and `value` (any JSON that TOML can hold).
- `unset`: `path` and `secret` (required; `true` when the setting is a secret). The key goes back to its default.
- `neo --dry-run settings patch` prints `DRY-RUN: set …` lines and writes nothing.
- The change waits for `neo activate`.

### Other settings commands

| Command | Does | On a server |
|---------|------|-------------|
| `neo edit` | Opens `configPath/settings.toml` in vim | Needs a terminal; prefer `settings patch` |
| `neo paste-settings` | Copies the loaded settings file (`--settings`, on a server `/etc/neo/settings.toml`) over `configPath/settings.toml` | **Do not.** It drops saved changes that are not activated yet |
| `neo git`, `neo lg` | lazygit in the config repo | Needs a terminal. Do not commit by hand |

## Containers

### `neo docker-update <CONTAINER>`

Pull the container's current image reference again and restart `docker-<CONTAINER>`. A `docker-` prefix in the name is dropped. Locks only that unit. For a newer version under a pinned tag, change the setting instead; for all containers, `sudo systemctl start neo-docker-updater`.

## Secrets

Run on the server. Each takes the system lock and rewrites files in the config repo: ask first.

| Command | Does |
|---------|------|
| `neo secrets split` | Moves secret settings from `settings.toml` into `secrets.yaml` (plain text) |
| `neo secrets enable` | Encrypts them into `secrets.sops.yaml` for this machine's SSH host key and the ssh-ed25519 `core.ssh.authorizedKeys`, and escrows the host key in `identity.sops.yaml` |
| `neo secrets disable` | Decrypts back into a plain `secrets.yaml` |
| `neo secrets rekey` | Re-encrypts for the current host key and authorized keys (after you edit `authorizedKeys`) |

## Backup, restore, new servers

### `neo backup rotate-key`

Encrypted backup, on the server: every encrypted destination gets a new key and a new full backup, then the old repository is deleted. Runs in the background as `backup-rotate-key.service`. Use it after you remove an admin key.

### `neo backup list --from-store <LOGIN>… [--key KEY]`

What a backup store holds, read over your normal SSH login to the store's server (`user@host`, `ssh://user@host:port` or an ssh_config alias): per server the format, versions, size, last backup, and which of your keys (`--key`, `~/.ssh/*.pub`, your agent) can restore it. Changes nothing. On the store's server the same comes from `neo backup-store list [--json]` (any user).

### `neo backup extract-config <BACKUP> [--out DIR] [--key KEY] [--backup-key KEY]`

Copy the config repo out of a backup (encrypted or plain files), e.g. when the server and its repo are gone. `BACKUP`: `user@host:folder`, `ssh://user@host:port/folder`, `https://name@store/folder` (backup store), `ssh+store://name@<login>/folder` (a backup store over SSH) or a local folder. Instead of `BACKUP`: `--from-store <LOGIN> [--client NAME]`. `--out` default `neo-config`. `--key`: admin ssh-ed25519 private key that opens the encrypted backup (default `~/.ssh/id_ed25519`). `--backup-key`: another key for the SSH login to the backup.

### `neo restore [REPO] [options]`

Rebuild a machine from its config repo and its backup. **It erases the target disk.** Run it on a NixOS installer as root, or from your computer with `--host`. Runbook: `docs/RESTORE.md`.

| Option | Meaning |
|--------|---------|
| `REPO` | Git URL, a checkout (its last commit), a bare repo, or a plain copy without `.git`. Leave it out with `--from-backup` to take the repo from the backup |
| `--host root@IP[:PORT]` | Restore that machine from here; nixos-anywhere boots it into an installer first when it runs another system |
| `--from-backup <BACKUP>` | Use this backup instead of the configured one (same forms as `extract-config`). Encrypted: opens with `--key` |
| `--from-store <LOGIN>` | A backup store over your SSH login to its server, for when its web address is down. Without `REPO`: lists its servers, picks one and takes the repo from its backup. With `REPO`: its store destinations are read over SSH first. Repeatable |
| `--client <NAME>` | With `--from-store`: the server to restore (default: the only one your keys open, else it asks) |
| `--store-via auto\|direct\|relay` | With `--host` and `--from-store`: the new machine logs in to the store server itself with your agent (`direct`), or the data goes through your computer (`relay`; keep the terminal open until the data is copied). `auto` (default): direct, then relay |
| `--disk <DISK>` | Disk to install onto (sets `disko.mainDisk`), e.g. `/dev/disk/by-id/…` |
| `--key <KEY>` | Admin ssh-ed25519 private key (default `~/.ssh/id_ed25519`) |
| `--backup-key <KEY>` | Another key that may log in to the backup |
| `--data backup\|none` | Data from the configured backup (default) or none (empty install) |
| `--data-from <DIR>` | Read a plain-files backup from this folder on the target (a mounted disk) |
| `--keep-hardware` | Keep the repo's `hardware-configuration.nix` |
| `--check` | Only check and print the plan |
| `--yes` | Do not ask before erasing the disk |
| `--reboot` | Reboot into the restored system when done |
| `--target <DIR>` | Root of the target system (default `/mnt`) |
| `--config <NAME>` | `nixosConfigurations` entry (default `neo`) |
| `--no-install` | Only clone the repo and restore the host key |
| `--neo <REF>` | Neo flake ref to run on the target (with `--host`) |

Always run `--check` first.

### `neo bootstrap <DIR> [options]`

Install Neo on a new server from your computer. A wizard asks for the server (`root@IP`), name, domain, logins, disk and whether the internet reaches it directly. If not, it adds a tunnel on a streamproxy edge (an existing Neo server, `homeserver@…`) and activates it there. It writes the config repo into `DIR`, then installs with `neo restore --data none` and waits for the web UI. Run it again with the same `DIR` to change answers or retry. Design: `docs/design/bootstrap.md`.

Flags (each answers one question): `--host root@IP[:PORT]`, `--key` (its `.pub` becomes an admin key), `--hostname`, `--domain`, `--email` (certificates), `--disk` (erased), `--public true|false`, `--edge [homeserver@]HOST[:PORT]`, `--server-neo-input` (e.g. `github:madebydamo/neo/dev`), `--web` (open `neo web` on the repo before the install), `--defaults` (ask nothing; fails when an answer is missing; the web login password can only be typed), `--no-install` (repo and edge only), `--neo` (Neo ref on the installer).

### `neo init`, `neo generate-hardware`, `neo nuke`

| Command | Does |
|---------|------|
| `neo init` | Creates the config repo at `configPath` from the template (with `repoUrl` as `origin`), writes `hardware-configuration.nix` and the settings, commits. Safe to run again |
| `neo generate-hardware` | Writes `configPath/hardware-configuration.nix` (`nixos-generate-config`, `--no-filesystems` when Disko is on) |
| `neo nuke` | Deletes everything in `configPath` except dotfiles (`.git`, …) and the `.neo-template` record. **Destructive.** Never on a server |

## Security scan

Only when `services.security` is on. `scan` and `triage` run as root (the timer and the web page run `neo-security-scan.service`).

| Command | Does |
|---------|------|
| `neo security scan [--no-ai] [--no-nix] [--no-images] [--system PATH] [--request FILE]` | Scans the system and every container image, then the AI review |
| `neo security triage [--all]` | Reviews the last scan again without scanning; `--all` also re-reviews findings with an AI verdict |
| `neo security show [--all]` | Prints the last report; `--all` includes harmless findings |
| `neo security accept <ID> [--reason TEXT]` | Marks a finding as not relevant here; kept across scans |
| `neo security reset <ID>` | Forgets the verdict; the next review looks again |
| `neo security serve` | Serves the report page (`neo-security-web`; `ROCKET_ADDRESS` / `ROCKET_PORT`) |

For a run on demand: `sudo systemctl start neo-security-scan`.

## Other commands

| Command | Does |
|---------|------|
| `neo web` | Starts the web UI against `configPath` (`ROCKET_ADDRESS` / `ROCKET_PORT`). On a server it already runs as `neo-web` on `127.0.0.1:8091` |
| `neo completions bash\|zsh\|fish\|elvish\|powershell` | Prints the tab completion script. The Nix package installs them already |
| `neo docker-ledger …`, `neo backup-store …` | Hidden, internal: the image ledger of the docker updater, the backup store's rescue portal (root). `neo backup-store list [--json]` runs as any user |

## On-server helpers (not `neo` subcommands)

| Command | Does |
|---------|------|
| `neo-restart-required [--json]` | Prints why a restart is due: `system` (boot default ahead of the running system), `kernel`, `docker` (dockerd runs an older `daemon.json`), `trigger:<id>` (a service asked for one). Exit 0 = restart due, 1 = not. JSON: `{"required","system","reasons"}` |
| `sudo neo-docker-rollback [--previous \| --at TIME] [--image REF]… [--unit UNIT]… [--no-restart]` | Returns containers to an earlier image. `--previous` (default): the version before the current one; without `--image`/`--unit` every image the last update changed. `--at TIME` (RFC 3339 or epoch): what ran then; without a filter, every image. `--no-restart`: retag only. Settings stay; the next updater run moves forward again. Older servers have another interface (`--all`, `--image`, `--manifest`): read its `--help` |
| `neo-restart-check record\|forget\|verify` | The check after an update restart; `verify` runs it now (the timer `neo-auto-update-verify` runs it after boot) |
| `neo-hermes-supervise system\|docker\|restart` | Has Hermes review the last system update, container update or restart (when `superviseUpdates` is on) |

The flags of `neo-docker-rollback` and the others come from the server's own scripts. Check with `--help` on the server (`neo-restart-required` ignores `--help` and prints its status).
