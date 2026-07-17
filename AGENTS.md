# AGENTS.md — Neo development guide

Concise map for humans and coding agents. Product overview: [README.md](README.md). Install: [docs/INSTALL.md](docs/INSTALL.md). Architecture: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## What this is

NixOS homeserver flake (**flake-parts** + **import-tree** over `./nix`) plus a Rust **`neo` CLI** (crane). Operators configure via `settings.toml`; modules turn that into OCI services, SWAG, volumes, and optional Disko/ZFS. No traditional unit-test suite—validate with `nix flake check`, `just` VM loop, and smoke checks over SSH.

## Architecture (what is special)

| Choice | Why it matters |
|--------|----------------|
| **Dendritic modules** | Every non-entrypoint `.nix` under `nix/` is auto-imported. One concern per file; no manual import list in root `flake.nix`. |
| **Service split** | `nix/services/<name>/{option,default,swag}.nix` — options, impl (`mkIf`), reverse-proxy. |
| **`lib.neo` helpers** | Activation scripts, reverse-proxy/auth, container image options, systemd unit lists, web UI helpers. |
| **settings.toml → neo** | Deploy flake maps TOML into `config.neo` (see `templates/homeserver/modules/settings.nix`). |
| **Plugins as flakes** | Extra inputs `plugin0…` export `nixosModules.default`; same option namespace. |
| **CLI + web UI** | `cli/`: clap commands; `neo web` edits TOML against live option schema. |
| **VM-first dev** | `just build` / `launch` / `ssh` against QEMU; `tools/id_ed25519` for SSH. |

## Repository map

| Path | Role |
|------|------|
| `flake.nix` | Inputs + `flake-parts` + `import-tree ./nix` |
| `nix/lib/` | `lib.neo` (activation, reverseProxy, containers, helpers, …) |
| `nix/modules/` | core, cli, disko, flakeparts |
| `nix/services/<name>/` | Built-in services |
| `nix/output/` | nixos configs, templates registration, devshell, systems |
| `cli/` | Rust `neo` + static/templates for web UI |
| `templates/homeserver` | `nix flake init` target for deploys (`#homeserver`) |
| `templates/plugin` | Skeleton for a plugin flake (`#plugin`) |
| `justfile` | format, check, VM build/launch/ssh |
| `build/` | Local init output (dev; gitignored-ish workflow) |
| `docs/` | INSTALL, ARCHITECTURE, CLI, PLUGINS |
| `settings.toml` | Local/dev operator settings (may contain secrets—do not commit real secrets) |

## Commands

```bash
# VM / integration
just build      # neo nuke → init → build (qcow / VM)
just launch     # shutdown + build + QEMU (SSH :2222)
just ssh        # root@localhost -p 2222
just exec CMD   # run CMD in VM
just logs SVC   # journalctl -b -u SVC
just status     # QEMU / monitor / SSH / disk
just shutdown

# Lint
just format     # alejandra .
just check      # nix flake check (root + build/)

# Nix / CLI
nix flake check -L
nix build .#neo
nix run .#neo -- --help
nix develop     # Rust/Cargo env with tools
```

**Rust:** `cargo fmt`, `cargo clippy -- -D warnings` (prefer via `nix develop`).

## Conventions

### Nix

- Files end in `.nix`; start with a short `#` comment; prefer under 200 lines.
- Service module pattern:

```nix
# Service description.
{ config, lib, ... }:
let
  cfg = config.neo.services.example;
in {
  imports = [ ./option.nix ./swag.nix ];
} // lib.mkIf cfg.enabled {
  # implementation
}
```

- Options: `enabled = mkEnableOption "…";`, snake_case attrs (`neo.services.*`), camelCase locals.
- Reverse proxy: `// neo.mkReverseProxyOptions { … }` (or `lib.neo…` where used that way).
- **WebSockets (SWAG):** `include /config/nginx/proxy.conf` already sets `Upgrade $http_upgrade` and `Connection $connection_upgrade`. **Do not** re-set those headers in `swag.nix` — nginx appends duplicates (`Upgrade: websocket, websocket`), uvicorn/Starlette returns **426**, and browsers show code **1006** / connection refused. Symptom: REST works, `POST /api/auth/ws-ticket` works, only `/api/ws` `/api/pty` `/api/events` fail. Debug: SWAG `access.log` status **426** + body “invalid Upgrade header”. Only set Upgrade/Connection yourself when the location does **not** include `proxy.conf`.
- Containers: `// lib.neo.mkContainerDefinitions { name = "image:tag"; };` then `image = cfg.containers.name;`.
  Extra non-docker units: `extraUnits = [ "my-setup" ];` inside `mkContainerDefinitions` (do not call `mkSystemdUnits` after it — that overwrites docker units).
- Units only (no containers): `// lib.neo.mkSystemdUnits [ "unit" ];`.
- Appdata (for web UI “Clear appdata”): `// lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/<name>";`.
- Hermes skill options: `// lib.neo.mkSkillOptions {};` on every service submodule (like reverse-proxy hooks).
- Volumes: `config.neo.core.volumes.appdata` (etc.), not hard-coded paths.
- Internal services: Docker `networks = ["internal"];`, `restart` always where applicable.
- Option UI helpers (tokens, bcrypt, mkpasswd): `helper = lib.neo.helpers.…` — see `nix/lib/helpers/`.

### Hermes skills (homeserver curriculum)

Teach Hermes via three layers (not one mega prompt):

| Layer | Where | Purpose |
|-------|--------|---------|
| SOUL.md | `$HERMES_HOME/SOUL.md` | Identity / safety (seeded once by Neo) |
| AGENTS.md | Hermes workspace | Always-on map: volumes, domain, enabled services, skill index |
| `/neo-*` skills | `skills.external_dirs` store tree | Per-service runbooks (CLI, architecture, credentials pointers) |

- Per service: `nix/services/<name>/skills.nix` sets `neo.services.<name>.skill.conf` when enabled (parallel to `swag.nix` → `proxyConf`).
- Collector: `nix/services/hermes/skills.nix` builds the skill tree, sets `services.hermes-agent.settings.skills.external_dirs`, force-writes workspace `AGENTS.md`, seeds SOUL.
- Helpers: `lib.neo.mkSkillOptions`, `mkSkillMd`, `mkServiceSkill`, `getSkillServices` in `nix/lib/skills.nix`.
- Prefer `mkServiceSkill { service = "…"; inherit cfg domain; description = "…"; body = ''…''; }` — auto-derives units, containers, subdomain, public URL, appdata, tinyauth, and **`meta.description`** (long product blurb for install guidance).
- Skill names: `neo-<service>` plus meta `/neo-homeserver`.
- Credentials: prefer settings keys + “where to create API tokens”; do not invent secrets Neo does not store.
- Plugins: ship `skills.nix` the same way; auto-imported with the plugin module.

### Rust (`cli/src/`)

- clap derive, `anyhow::Result`, `?` + `.context(…)`, no `unwrap()` in library paths.
- `toml_edit::DocumentMut` for surgical TOML edits.
- Commands in `cli/src/commands/`; web under `commands/web/`.

## Workflow: add a service

1. `nix/services/<name>/option.nix` — submodule + proxy/containers/units/meta + `mkSkillOptions` as needed.
2. `nix/services/<name>/default.nix` — `mkIf`, activation helpers, oci-container.
3. `nix/services/<name>/swag.nix` if publicly proxied.
4. `nix/services/<name>/skills.nix` — Hermes skill (`neo-<name>`) with CLI, architecture, credentials guidance.
5. New volume only if required → `nix/modules/core/`.
6. `just format && just check`
7. Optional: `just build && just launch` → `just exec 'systemctl status …'`

Plugins: same patterns inside a separate flake — [docs/PLUGINS.md](docs/PLUGINS.md).

## Verification checklist

- [ ] `just format` / `just check`
- [ ] Plain-string option descriptions; `lib.neo` helpers for dirs/files/proxy
- [ ] Dendritic split respected; volumes via `neo.core.volumes.*`
- [ ] No secrets committed; internal network for private containers
- [ ] VM smoke test when behavior changes

## Security / agent don'ts

- Do not commit secrets or real API keys.
- Prefer ports above 1024 for non-root listeners; least privilege.
- Do not force-push or commit unless the user asks.
- Prefer fixing root causes over bypassing checks.

## Debug

```bash
just status
just logs <svc>
just exec 'journalctl -u <svc> --no-pager -f'
just exec 'ls -la /var/neo/DATA/AppData/<svc>'
nix flake check --print-build-logs
```

CLI details: [docs/CLI.md](docs/CLI.md).
