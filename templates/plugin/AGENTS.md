# AGENTS.md

This repo is a **Neo plugin**: a Nix flake that adds services to a Neo homeserver (github:madebydamo/neo). Neo imports every `neoPlugin.modules.*` under `modules/` into the host, once per copy of the plugin.

## Before you change anything

1. Use the `neo-plugin` skill if you have it (Claude Code: `/plugin install neo --marketplace madebydamo/neo`; this repo's `.claude/settings.json` offers it).
2. Read the docs of the Neo revision this plugin pins, not GitHub `master`:

   ```bash
   NEO=$(nix flake archive --json 2>/dev/null | jq -r .inputs.neo.path)
   ```

   `$NEO/docs/PHILOSOPHY.md` (why), `$NEO/docs/PLUGIN-DEVELOPMENT.md` (how, with the checklist), `$NEO/docs/LIB.md` (every `lib.neo` helper), `$NEO/nix/services/*` (working examples).

## Layout

| Path | Role |
|------|------|
| `flake.nix` | Generated. Do not edit; change `modules/inputs.nix` and run `nix run .#write-flake` |
| `modules/imports.nix` | Imports `inputs.neo.flakeModules.plugin` |
| `modules/services/<svc>/option.nix` | Options under `neo.services.<name>`: what the operator sets in the web UI |
| `modules/services/<svc>/default.nix` | What runs (containers, units), inside `lib.mkIf cfg.enabled` |
| `modules/services/<svc>/swag.nix` | HTTPS vhost via `lib.neo.mkSubdomainProxyConf` |
| `modules/services/<svc>/skills.nix` | Skill for Neo's assistant via `lib.neo.mkServiceSkill` |
| `modules/services/<svc>/monitoring.nix` | Metrics endpoints and a Grafana dashboard (`neo.monitoring.apps`, `lib.neo.grafana`) |

## Rules

- Every file under `modules/` is auto-imported. Never `imports = [ ./sibling.nix ]`. `git add` new files: flakes ignore untracked ones.
- `name = plugin.name "<svc>"` for every service key, container, unit, appdata dir, subdomain, user and secret name. Titles get `${plugin.label}`.
- All state under `"${config.neo.core.volumes.appdata}/${name}"` (`mkAppdata`, `mkEnsureDirs`). Nothing in `/var/lib`.
- Secrets: `lib.neo.mkSecretOption`, passed only through `neo.core.secrets.containerEnv` / `.templates`. Never in `environment` or scripts.
- Containers on `networks = [(lib.neo.docker.serviceNetwork name)]` (each service its own network; another service only through its public URL or `lib.neo.docker.mkLink`), no published web ports. Health/API paths called without a login go in `auth.publicPaths`.
- Every unit in `mkContainerDefinitions { extraUnits; }` or `mkSystemdUnits`; post-start config with `lib.neo.mkSetupService`.
- Metrics and dashboards go in `neo.monitoring.apps.${name}`; queries filter with `lib.neo.grafana.appSelector name`; exporters only on with `config.neo.monitoring.collecting`.
- Texts are for non-technical operators: `tagline`/`description` say what they get, technical facts go in `notes` or Nix comments.
- Options are API: operators have them in `settings.toml`. Add, do not rename.

## Check

```bash
nix fmt && nix flake check      # neo-plugin-instances: fails on names that skip plugin.name
```

Then run it on a Neo VM or host: `$NEO/docs/PLUGIN-DEVELOPMENT.md` → Development loop and Verify on the host.
