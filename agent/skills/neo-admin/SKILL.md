---
name: neo-admin
description: Operate a running Neo homeserver (github:madebydamo/neo) over SSH as homeserver@<host>. Use when asked to check, debug, fix, configure, update, roll back or restart a Neo server or one of its apps, to read its logs, units, containers, update runs, backups, settings, ZFS pools or snapshots, or to look up a `neo` CLI command or flag (activate, update, generation, settings get/patch, docker-update, secrets, backup, restore, bootstrap, security) or an on-server helper (neo-restart-required, neo-docker-rollback). To change Neo's own source use neo-core; to write a plugin use neo-plugin.
---

# Operating a Neo server

A Neo server is one NixOS machine. Its owner sets **settings** (web UI or `neo settings patch`). **`neo activate`** builds them into a new NixOS **generation** and switches to it. Apps run as Docker containers (`docker-<container>.service`) or native units behind SWAG (HTTPS) and tinyauth (login). All app data is under `/var/neo/DATA/AppData/<service>`, outside the generations.

You work from another computer over SSH. The web UI does the same things for people: when the owner is at hand, a web UI button is often the better way.

The full command reference is [references/cli.md](references/cli.md). Read it before you run a `neo` command you do not know.

## 1. Connect

- Log in as **`homeserver@<host>`**. `neo` runs as this user. It owns the config repo and `/etc/neo/settings.toml`. An ssh_config alias works: `ssh homeserver@hattori` keeps the alias's host name and ports, and `homeserver@` replaces its `User`.
- `admin@<host>` uses the same keys, but `neo` there re-runs itself with `sudo -u homeserver`, which asks for a password. Use `homeserver@`.
- Run each command non-interactively: `ssh homeserver@HOST 'cmd'`, or `ssh homeserver@HOST bash -s < script.sh` for a longer read-only script. Do not start `neo edit`, `neo git`, `neo lg` or a foreground `neo web`: they need a terminal.
- `homeserver` is in the groups `wheel`, `docker` and `neo-git`. `docker …` and `journalctl …` work without sudo.
- Passwordless sudo is limited to: `systemctl`, `journalctl`, `systemd-run`, `nixos-rebuild`, `switch-to-configuration`, `nix-store`, `nix-env`, `nix-collect-garbage`, `rm`, `neo-docker-rollback` (newer servers), `neo-git-ssh-key`, `neo-secrets-root`, and on a ZFS machine also `zfs` and `rsync` (`sudo -n -l` lists them). Every other `sudo` (`zpool`, `cat` of a root file, …) asks for a password. Then stop and ask the user to run it. Never send a password over SSH yourself.
- These rules exist for the web UI and the updaters, and they accept any arguments. Use `sudo` only for the commands this skill names, and only after the user agrees when the command changes something.

## 2. Match the server's version

Each server follows its own Neo branch (`neo-cli.server.neoInput`, e.g. `github:madebydamo/neo` or `github:madebydamo/neo/dev`) and updates on its own schedule. Commands and flags differ: an older `neo` has no `settings`, `bootstrap`, `completions` or `--config-path`, and an older `neo-docker-rollback` takes other flags.

- If you are not sure that a command or flag exists there, run `ssh homeserver@HOST 'neo <command> --help'` first. The server's own help wins over this skill.
- Which Neo the server runs: `grep -E '^\s*neoInput' /etc/neo/settings.toml` and `neo --version`.
- Its exact Neo source (to read an option or a unit): `cd <configPath> && nix flake archive --json | jq -r .inputs.neo.path` prints a `/nix/store/…-source` path on the server. Read `nix/services/<name>/option.nix` there for the settings of a service.

## 3. First look (read-only)

```bash
ssh homeserver@HOST '
  hostname; neo --version
  systemctl --failed --no-legend                     # empty = no failed unit
  neo-restart-required                               # exit 0 = a restart is due; --json for scripts
  neo generation list | head -3                      # "(current)" marks the running one
  systemctl list-timers --no-legend | grep -E "neo|backup"
  docker ps -a --filter status=restarting --filter status=exited --format "{{.Names}} {{.Status}}"
  jq -c "{started_at,changed,failed,exit_code,restart_reasons}" /var/neo/DATA/AppData/updater/system/last.json
'
```

### Where things are

| What | Where |
|------|-------|
| Settings of the running system (written at each activation) | `/etc/neo/settings.toml` (0600, homeserver) |
| Config repo: saved settings, `flake.nix`, `flake.lock`, `modules/`, `secrets.sops.yaml`, `identity.sops.yaml` | `neo-cli.server.configPath`, default `/var/neo/DATA/AppData/configuration` |
| App data | `/var/neo/DATA/AppData/<service>` (volumes `core.volumes.*`, root `/var/neo`) |
| Nightly system update runs | `/var/neo/DATA/AppData/updater/system/` (`<utc>-<pid>.json` + `.log`, `last.json`) |
| Container update runs | `/var/neo/DATA/AppData/updater/docker/` (same layout) |
| Activation and update logs (CLI and web UI) | `/tmp/neo-activations/activation_<suffix>.{log,json}`, `update_<suffix>.{log,json}` |
| Which image ran when | `/var/lib/neo/docker-images.json` |
| Operation locks | `/run/neo/locks/` (`system.lock`, `unit_docker-<c>.service.lock`) |
| Backup log | `journalctl -u backup`, `/var/log/backup-<destination>.log` |
| Web UI | `https://neo.<domain>` (behind tinyauth), or `ssh -N -L 8091:127.0.0.1:8091 homeserver@HOST` and then `http://127.0.0.1:8091` |

### Units

| Unit | Job |
|------|-----|
| `neo-web` | Web UI on `127.0.0.1:8091` |
| `neo-bootstrap` | Sets up the config repo; the web UI needs it |
| `neo-auto-update` (`.timer`) | System updater: `neo update`, then `neo activate`, then the restart decision |
| `neo-auto-update-verify` (`.timer`) | Checks the boot after an update restart (`restart_check` in the run JSON) |
| `neo-docker-updater` (`.timer`) | Pulls newer images, restarts changed containers |
| `neo-activate@<suffix>`, `neo-update@<suffix>` | Activations and updates started from the web UI (transient) |
| `backup` (`.timer`) | Backup to every destination |
| `neo-security-scan` (`.timer`) | Vulnerability scan, when `services.security` is on |
| `neo-docker-network` | Creates the Docker networks |
| `neo-<service>.target` | Groups every unit of one service: `systemctl list-dependencies neo-immich.target` |
| `docker-<container>` | One container |

## 4. Settings: two files

- `<configPath>/settings.toml` holds the **saved** settings. A web UI Save and `neo settings patch` write there. The change waits there until the next activation.
- `/etc/neo/settings.toml` holds the settings the **running** system was built from.
- Pending changes: `diff -q /etc/neo/settings.toml <configPath>/settings.toml`. Use `-q`: a full diff can show secret values.

Change a setting:

1. Read it: `neo settings get services.immich.enabled` (JSON, `null` when unset). Settings paths are the option paths without `neo.`: `services.<name>.<option>`, `core.<option>`.
2. Write it. Test with `--dry-run` first:

   ```bash
   echo '[{"set":{"path":["services","searxng","enabled"],"value":true}},
          {"unset":{"path":["services","pihole","localIP"],"secret":false}}]' \
     | neo --dry-run settings patch
   ```

   `set` takes `path` and `value`. `unset` takes `path` and `secret` (required; `true` for a secret setting). The command takes the same lock as a web UI save.
3. Apply it with `neo activate` (section 5), after the user agrees.

Rules for settings:

- Never edit `settings.toml` with `sed` or an editor. Never run `neo paste-settings` on a server: it copies `/etc/neo/settings.toml` over the saved settings and drops every pending change.
- Never put a secret value on a command line or in your context. The owner sets secrets in the web UI (most have a Generate button). `neo settings get` hides values kept in a secrets file, but shows a secret that is still inline in `settings.toml`: do not read secret keys.
- The config repo is a git repo, but never commit, reset or push in it by hand. Activations and the nightly update commit there.

## 5. Apply, update, go back

Each of these changes the running system. Say what it does and get a yes from the user first.

| Goal | Command | Note |
|------|---------|------|
| Apply the saved settings | `neo activate` | Live switch. Takes minutes; use a long SSH timeout. If SSH drops, follow `/tmp/neo-activations/activation_<suffix>.log` |
| Apply at next boot only | `neo activate --mode boot` | The running system stays as it is |
| See what activate would do | `neo --dry-run activate` | Changes nothing |
| Update Neo, NixOS and plugins | `neo update && neo activate` | Same as the nightly run, without its restart logic |
| Run the nightly update now | `sudo systemctl start neo-auto-update` | Full run with history and Hermes review. It can **restart the server** about a minute later when a restart is due |
| Go back to an older system | `neo generation list`, then `neo generation switch N` (live) or `neo generation boot N` (next boot) | Data is not touched. The config repo is not reverted, so the next activation or nightly update builds the newer settings again: fix the cause too |
| Re-pull one container | `neo docker-update <container>` | Same image reference, restarts `docker-<container>` |
| Undo a container update | `sudo neo-docker-rollback --previous --image '<repo:tag>'` | Older servers: read `neo-docker-rollback --help` first |
| Containers as they ran at a time | `sudo neo-docker-rollback --at <RFC 3339 or epoch> [--unit docker-x]` | E.g. after a data restore |
| Restart the server | `neo-restart-required` (reasons), then `sudo systemctl reboot` | Ask first. Web UI: Settings → Power |

Only one change runs at a time. A conflict fails at once with the holder, e.g. `Error: Blocked: Activation in progress (started 12:03)`. Wait with `--lock-wait 600`, or find out what runs: `systemctl list-units --no-legend 'neo-activate@*' 'neo-update@*' neo-auto-update.service neo-docker-updater.service`.

Never run `neo activate` on your computer against the server's config, never `neo nuke` or `neo init` on a server, and never `neo restore` against a server that runs: restore erases the disk.

## 6. ZFS (Disko layout only)

Check first: `findmnt -t zfs -n -o TARGET,SOURCE`. Empty output means no ZFS: the machine has no snapshots and no data restore, so skip this section.

Layout:

| Dataset | Mounted at | Snapshots |
|---------|-----------|-----------|
| `zroot/root` | `/` (with `/nix`) | Never. The system comes back through generations |
| `zroot/neo` | `/var/neo`: all Neo data and the config repo | Yes: automatic and manual |
| `zpool-<disk>/…` | One pool per `disko.additionalDisks` entry (e.g. `/dev/mmcblk0` → `/mnt/mmcblk0`) | None |

Read-only commands. They need no sudo, because `/dev/zfs` is open to every user:

```bash
zpool list -o name,size,alloc,free,frag,cap,health
zpool status -x                                    # "all pools are healthy"
zfs list -o name,used,avail,mountpoint
zfs list -t snapshot -o name,used,creation,userrefs,neo:comment -s creation zroot/neo
zpool get -H -o value neo:restore,neo:restore-result zroot   # "-" = no data restore pending / none done
ls /var/neo/.zfs/snapshot/<snap>/DATA/AppData/<service>/     # read an old file straight from a snapshot
```

Snapshot names on `zroot/neo`:

| Name | Made by | Deleted |
|------|---------|---------|
| `zfs-auto-snap_<frequent\|hourly\|daily\|weekly\|monthly>-<date>` | zfstools timers `zfs-snapshot-<interval>.timer` | Automatically. Keeps `disko.autoSnapshot.*`: 4 frequent (15 min), 12 hourly, 7 daily, 4 weekly, 3 monthly by default; `0` turns an interval off |
| `neo-data-<utc>` | Web UI, Versioning → Data snapshots → Snapshot now | Never automatically |
| `neo-snap-<service>-<utc>` | Web UI, a service's Snapshots | Never automatically |
| `neo-prerestore-<service>-<utc>`, `neo-prerestore-<ts>` | The safety snapshot before a service or data restore | Never automatically |
| `neo-backup-<utc>` | `backup` for each run; with `format = "zfs"` the last one sent stays as the next incremental base | By `backup` |

A **pinned** snapshot has the `zfs hold` tag `neo-pin` (`userrefs` > 0). zfstools then only marks it for deletion, and it stays until it is unpinned. The label shown in the web UI is the user property `neo:comment`.

Restores (both from the web UI; never by hand):

- **One service** (service page → Snapshots): the web UI stops `neo-<service>.target`, takes `neo-prerestore-<service>-…`, copies the service's appdata folder back from `.zfs/snapshot/<snap>/` with rsync, then starts the service again.
- **All Neo data** (Versioning → Data snapshots): the web UI sets `neo:restore` on the pool and restarts the server. In the next boot the initrd unit `neo-zfs-restore` swaps `zroot/neo` for a clone of the snapshot. It uses no `zfs rollback`. The old state stays as `zroot/neo.prev-<ts>` with its newer snapshots, and you can restore it like any snapshot. The result is in `neo:restore-result` (`ok|…` or `failed|…`) and in the web UI banner, which also offers to return the apps to the image versions that ran then.

Rules for ZFS:

- Even though sudo allows `zfs` with any arguments, never run `zfs destroy`, `rollback`, `rename`, `promote`, `set`, `inherit`, `hold` or `release` by hand. The web UI takes the operation lock, the safety snapshot and the restart for you. A wrong `destroy` loses data that no generation brings back.
- Neo turns on no automatic scrub: `zpool status` shows no `scan:` line until the first one. `sudo zpool scrub zroot` needs the password, so ask the user.

## 7. Debug recipes

| Symptom | Look at |
|---------|---------|
| App down, 502 or blank | `systemctl list-dependencies neo-<svc>.target`; `systemctl status docker-<c>`; `journalctl -u docker-<c> -b -n 100 --no-pager`; `docker logs --tail 100 <c>` |
| Is the app itself up? | `docker exec swag curl -sS -m 10 -o /dev/null -w '%{http_code}\n' http://<c>:<port>/` (SWAG reaches every proxied app) |
| Proxy or login wrong | From your computer: `curl -sk -m 20 -D - -o /dev/null https://<sub>.<domain>/` (302 to `tinyauth.<domain>` = proxy and login work). Vhost: `docker exec swag cat /config/nginx/proxy-confs/<sub>.subdomain.conf`. Errors: `docker logs swag 2>&1 \| grep -iE 'emerg\|error' \| tail` |
| Every site down | SWAG: `systemctl status docker-swag`, `docker logs --tail 50 swag`. An `emerg` in one vhost stops nginx for all |
| Two apps cannot talk | By design each service has its own network: `docker exec <c> getent hosts <other> \|\| echo isolated`. A link is declared in Nix (`mkLink`), not added by hand |
| Activation failed | The log in `/tmp/neo-activations/`; `systemctl --failed`; the config repo stays on the `activation_<suffix>` branch for inspection |
| Nightly update failed | `jq . /var/neo/DATA/AppData/updater/system/last.json`, the `.log` next to it, `journalctl -u neo-auto-update -b --no-pager` |
| A container update broke an app | `/var/neo/DATA/AppData/updater/docker/last.json` and `.log`; then roll back (section 5) |
| "Restart required" in the UI | `neo-restart-required --json`: `system`, `kernel`, `docker` or `trigger:<id>` |
| Backup failed | `systemctl status backup`; `journalctl -u backup -b --no-pager` (lines tagged with the destination); `/var/log/backup-<destination>.log` |
| Disk full | `df -h / /nix /var/neo`; `du -xsh /var/neo/DATA/AppData/* \| sort -h \| tail`. Old generations: `sudo nix-collect-garbage --delete-older-than 30d` (ask first). On ZFS, `df` hides what snapshots hold: `zfs list -o name,used,usedbysnapshots,avail` and the largest snapshots with `zfs list -t snapshot -o name,used -s used \| tail` (section 6; delete them in the web UI) |
| Pool degraded or errors | `zpool status -v`; report it to the user at once (disk failing). Do not try to repair |
| Command says `Blocked` | Section 5, the lock paragraph |

Start with reading. Change one thing, then check again. Report what you saw, with the command output.

## 8. Rules

- Ask before every change that is hard to undo: activate, update, generation switch or boot, reboot, `docker-update`, rollback, `settings patch`, starting `backup` or an updater unit, anything under `neo secrets` or `neo backup rotate-key`, and Clear appdata or snapshot restores in the web UI.
- Never edit `/nix/store`, SWAG proxy-confs or containers by hand for a lasting fix: the next activation writes them again. A lasting fix is a setting, or a change to Neo or the plugin (skills neo-core, neo-plugin).
- Never print secrets: do not `cat` `settings.toml`, `secrets.yaml` or `*.sops.yaml`, do not `docker inspect` or `env` a container that holds secrets, and do not run `neo-secrets-root` or `neo-git-ssh-key` (they decrypt secrets and keys).
- Plugin services behave like built-in ones. Helper scripts a plugin installs (other `neo-*` names) are documented by that plugin.
- Design background, when a recipe is not enough: in a Neo checkout, `docs/ARCHITECTURE.md`, `docs/RESTORE.md` and `docs/design/` (`restart-required`, `backup-restore`, `secrets`, `operation-locks`, `network-isolation`).
