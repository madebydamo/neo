---
name: neo-core
description: Change Neo itself, the NixOS homeserver flake and Rust `neo` CLI/web UI (github:madebydamo/neo). Use when working in a Neo checkout (flake with nix/services, nix/lib, cli/, templates/homeserver) on a built-in service, lib.neo helper, web UI page or widget, secrets, backup/restore, disko/boot, Hermes assistant integration or guides, or when asked how Neo is designed. For plugins (separate flakes using neoPlugin.modules) use the neo-plugin skill.
---

# Working on Neo

Neo is a NixOS homeserver for non-technical owners: `settings.toml` → `config.neo` → OCI containers and native services behind SWAG (HTTPS) and tinyauth (login), managed from a web UI served by the Rust `neo` CLI. This skill is the entry point; the repo's own docs are the source of truth.

## 1. Get the source and the rules

Work in a checkout of `github:madebydamo/neo` (`git clone https://github.com/madebydamo/neo`). Then read:

| File | When |
|------|------|
| `AGENTS.md` | Always, first. Repo map, `just` recipes, how to add a service, web UI metadata, locks, secrets, smoke test, don'ts |
| `docs/PHILOSOPHY.md` | Always, once. Why the rules exist; decide unclear cases by it |
| `docs/ARCHITECTURE.md` | Traffic, login and update model in plain words |
| `docs/PLUGIN-DEVELOPMENT.md`, `docs/LIB.md` | Anything touching `lib.neo` or the plugin interface (plugins are its public API) |
| `docs/design/<topic>.md` | Before changing that area: `secrets`, `backup-restore`, `plugin-instances`, `descriptions-and-guides` |
| `nix/services/<similar>/` | Before writing a service: copy the shape of the closest existing one |

## 2. Route the task

| Task | Start at | Must also |
|------|----------|-----------|
| New or changed built-in service | `AGENTS.md` → Add a service; `nix/services/<similar>/` | Texts per AGENTS.md option/service text rules; `skills.nix`; enable in `settings.toml`; VM smoke test |
| `lib.neo` helper | `nix/lib/<file>.nix` | Update `docs/LIB.md` (it claims to be complete); keep existing call sites and plugins working |
| Web UI page / option form | `cli/src/commands/web/`, `cli/templates/` | No service or option names in UI code: metadata on the option (`ui`, `rank`, `helper`); `nav_progress.js` for loading, `LockSpec` + `data-neo-lock` for mutations |
| New composite editor | `AGENTS.md` → Adding a new special UI | Widget partial + JS + `*.test.js`, `just test-widgets`, document it in AGENTS.md |
| Settings write path | `settings::edit::apply_edit` | One write path for form, proposals and CLI; never a second one |
| Secrets | `docs/design/secrets.md`, `nix/modules/core/secrets.nix` | Never a secret in `environment`, `/etc`, scripts or eval-time hashes |
| Backup / restore | `docs/design/backup-restore.md`, `nix/services/backup`, `cli/src/commands/restore/` | Change the backup and restore sides together; update `docs/RESTORE.md` |
| Disko / boot loader | `nix/modules/disko/`, `core/base.nix` | `just test-disko-boot` (`just launch` never tests the boot loader); boot scripts must never fail a switch |
| Plugin interface | `nix/lib/plugin.nix`, `nix/output/plugin-flake-module.nix`, `templates/plugin` | The id/label rules exist in three places plus widget JS; keep them equal. Update `docs/PLUGIN-DEVELOPMENT.md` and the `neo-plugin` skill |
| Guide | `nix/guides/<id>/`, `AGENTS.md` → Guides | One outcome, real settings links, non-technical reader |
| Assistant (Hermes) | `nix/services/hermes/`, `AGENTS.md` → Assistant chat | Proposals write nothing until the user saves; secrets never pass through the agent |

## 3. Loop

1. `git add` new files (flakes only see tracked files).
2. `just launch` (rebuild + boot the QEMU VM), wait for `just exec 'echo up'`.
3. Smoke test from `AGENTS.md` → Live VM smoke test: unit active, internal curl from SWAG, public health 200, UI 302 to tinyauth, no SWAG `emerg`. Web UI changes: `just web` and look at the page.
4. `just format && just check` (also `just test-widgets` for UI JS).
5. Commit in the repo style, one change per commit: `<area>: <what changed>` (`web:`, `nix:`, `docs:`, `cli:`, `guides:`, `hermes:`, `backup:`, `disko:`, a service name …).

## 4. Keep agents and docs in sync

Neo's docs are written for coding agents as much as for people. When you change a rule, a helper signature, a check or an error message, update the doc that states it in the same commit: `AGENTS.md`, `docs/LIB.md`, `docs/PLUGIN-DEVELOPMENT.md`, and the skills in `agent/skills/` when they repeat it. Prefer a check (assertion, flake check, test) over a sentence in a doc.
