---
name: neo-plugin
description: Write, port or debug a plugin for Neo, the NixOS homeserver (github:madebydamo/neo). Use when a repo imports inputs.neo.flakeModules.plugin or defines neoPlugin.modules, when asked to "turn this project into a Neo plugin" or "add a service to a Neo plugin", or when working with lib.neo helpers (mkReverseProxyOptions, mkContainerDefinitions, mkServiceMeta, mkSetupService, mkSecretOption), plugin.name prefixes, or the neo-plugin-instances check.
---

# Neo plugin development

A Neo plugin is a Nix flake that adds services (or plain NixOS config) to a Neo homeserver. Its services get Neo's HTTPS, login, web UI, updates, status, backups and AI assistant for free, as long as they follow Neo's shape. This skill is the map; the details live in Neo's own docs.

## 1. Read the Neo source your plugin builds against

Never guess Neo APIs from memory or from GitHub `master`. The plugin's `flake.lock` pins a Neo revision, and its source is in the Nix store:

```bash
NEO=$(nix flake archive --json 2>/dev/null | jq -r .inputs.neo.path)   # in the plugin repo; fetches the inputs
# Inside a Neo checkout: NEO=$PWD
# No Nix at hand: https://raw.githubusercontent.com/madebydamo/neo/master/<path>
```

No `$NEO/docs/PLUGIN-DEVELOPMENT.md`? The plugin pins a Neo from before these docs: run `nix flake update neo` (the host builds against its own, newer Neo anyway), or read them from GitHub.

Then read, in this order and only as far as the task needs:

| File | When |
|------|------|
| `$NEO/docs/PHILOSOPHY.md` | Always, once. The reasons behind every rule below; decide unclear cases by it |
| `$NEO/docs/PLUGIN-DEVELOPMENT.md` | Always. Layout, prefixes, dev loop, **the checklist**, annotated `option`/`default`/`swag`/`skills.nix`, other service shapes (native module, static site, self-built image), texts, guides, privacy map, verification |
| `$NEO/docs/LIB.md` | Looking up a `lib.neo` function's arguments or a `config.neo` value. Grep it, do not read it whole |
| `$NEO/templates/plugin/` | Starting point (`nix flake init -t github:madebydamo/neo#plugin`) |
| `$NEO/nix/services/<name>/` | A working example of a pattern: `vikunja` (native NixOS module in appdata), `paperless` (database sidecars, secrets, assertions), `syncthing` (setup unit, protocol ports), `nextcloud` (setup units), `backup` (timers) |
| `$NEO/nix/lib/*.nix` | The source of truth when the docs and a helper disagree |

If this skill and `$NEO` disagree, `$NEO` wins: it is the version the plugin builds against.

## 2. Workflow

**New plugin:** `nix flake init -t github:madebydamo/neo#plugin`, `git init && git add -A`, rename `example` (directory, module names, every `plugin.name "example"`), then work through the checklist in `PLUGIN-DEVELOPMENT.md` item by item.

**Turn an existing project into a plugin:** keep the project's own build (`perSystem.packages`), add the template's `modules/imports.nix` / `inputs.nix` / `nixos.nix`, add one `modules/services/<name>/` per service. Prefer the upstream OCI image or nixpkgs module; build an image only when there is none (`PLUGIN-DEVELOPMENT.md` → Other service shapes).

**Port an old plugin** (`flake.modules.nixos.*`, no prefixes): `PLUGIN-DEVELOPMENT.md` → Prefixes → "Porting an existing plugin".

**Every change:** `git add` new files → `nix fmt` → `nix flake check` → run it on Neo (section 4).

## 3. Rules that are easy to get wrong

Each one is enforced by a check or breaks something an operator relies on.

- **One file per concern, all auto-imported.** `modules/services/<svc>/{option,default,swag,skills}.nix`, each defining `neoPlugin.modules.<svc>[-option|-swag|-skills]` (unique names). Never `imports = [ ./sibling.nix ]`. Files starting with `_` are skipped (helpers, data).
- **Every name through `plugin.name`.** `name = plugin.name "<svc>";` then use `name` for the `neo.services` key, containers, sidecars (`"${name}-db"`), units, appdata, subdomain, users, secret templates, guide ids and `service:` links. Titles get `${plugin.label}`. Core services (`swag`, `tinyauth`) keep their names. `nix flake check` (`neo-plugin-instances`) fails on any miss.
- **Git sees only tracked files.** An untracked icon, script or module is silently missing. `git add` before every build.
- **All state in appdata.** `"${config.neo.core.volumes.appdata}/${name}"`, declared with `mkAppdata`, created with `mkEnsureDirs` in `preStart` (or a tmpfiles rule). Never `/var/lib/<name>`, never `DynamicUser`/`StateDirectory`: native services run as `User`/`Group = "homeserver"` with `ReadWritePaths`.
- **Secrets never in the store.** Declare with `lib.neo.mkSecretOption` (+ `helper = lib.neo.helpers.randomToken`, + an assertion that it is set). Hand them over only via `neo.core.secrets.containerEnv.<container>` or `neo.core.secrets.templates`. Never in `environment`, a script, `/etc` or a hash computed at eval (`lib.neo.secrets.sha256`/`base64`/`jsonString` instead).
- **One front door.** Containers on `networks = ["internal"]`, no published web ports, `swag.nix` via `lib.neo.mkSubdomainProxyConf`. In hand-written vhosts never re-set `Upgrade`/`Connection`/`proxy_*_timeout` (one duplicate takes down every vhost). Anything called without a browser login (health, API, mobile app, webhooks) goes in `auth.publicPaths`.
- **Every unit listed.** `mkContainerDefinitions { ${name} = "<image:tag>"; extraUnits = [...]; timerUnits = [...]; }`, or `mkSystemdUnits` for native services. Post-start configuration is `lib.neo.mkSetupService`, listed in `extraUnits`.
- **Everything under `config = lib.mkIf cfg.enabled { ... }`.**
- **Texts for non-technical operators.** `mkServiceMeta { tagline; description; notes; replaces; privacy; license; }` and option descriptions say what the person gets, in plain words; technical detail in `notes` or Nix comments; `ui.label` on non-obvious keys; plumbing in `ui.groups.advanced` or `internal = true`. Rules: `PLUGIN-DEVELOPMENT.md` → Texts for the web UI.
- **Be honest about licenses.** `license = { spdx; url; }` for the upstream project; `openSource = false` + `note` when the image contains closed code or the service needs a closed server.
- **Settings are API.** Every option is a key in operators' `settings.toml`; renaming or removing one breaks their evaluation. Add options, do not rename them.
- **Use the helper.** Sudo (`mkSudoExtraRules`), git (`neo.core.git.users`), host listeners (`127.0.0.1` + `mkDockerToLocalhostForward`), UI widgets (generic `ui` metadata; a new widget belongs in Neo, not the plugin).
- **Ship a `skills.nix`** (`mkServiceSkill`) so Neo's assistant can operate the service. Name only settings keys, units and paths that exist.

## 4. Verify

```bash
nix fmt && nix flake check                     # format + neo-plugin-instances
```

On a Neo VM (needs a Neo checkout; `PLUGIN-DEVELOPMENT.md` → Development loop): add `[core.plugins.<id>] url = "git+file:/abs/path/to/plugin"` and `[services.<name>] enabled = true` to its `settings.toml`, then `just launch`, then:

```bash
just exec 'systemctl is-active docker-<name> docker-swag docker-tinyauth'
just exec 'docker exec swag curl -sS -m 10 http://<name>:<port>/<health>'            # internal
just exec 'docker logs swag 2>&1 | grep -i emerg'                                     # must be empty
curl -sk -m 20 -w "\n%{http_code}\n" "https://<subdomain>.<domain>/<health>"          # 200 (publicPaths)
curl -sk -m 20 -D - -o /dev/null "https://<subdomain>.<domain>/"                      # 302 to tinyauth
```

Done means: units active, internal OK, public health 200, UI 302 to tinyauth, no SWAG `emerg`, and the service shows on the web UI's Services grid with icon, tagline and its units.

## 5. Errors and their fix

| Message (excerpt) | Fix |
|-------------------|-----|
| `This plugin cannot be added twice: these names do not go through plugin.name` | Replace the listed literals with `name` / `plugin.name "…"` |
| `images: … (the same image name in every copy …)` | Load self-built images as `"${name}:<tag>"` |
| `Setup units built with lib.neo.mkSetupService are not listed …` | Add the unit to `extraUnits` (or `mkSystemdUnits`) in `option.nix`, define it inside `mkIf cfg.enabled` |
| `a secret placeholder would be used literally in …` | Move the secret from `environment`/script to `neo.core.secrets.containerEnv` or `.templates` |
| `The option 'neo.services.<x>.<y>' does not exist` | Option renamed/removed, or the file declaring it is not `git add`ed |
| `does not support a prefix` / `still has modules that cannot take a prefix` (on the host) | The plugin has no `neoPlugin`, or still has `flake.modules.nixos.*` modules: port them to `neoPlugin.modules` |
| `attribute 'neo' missing` in the outer `{inputs, ...}:` function | `lib.neo` exists only inside NixOS modules; use `inputs.neo.lib.neo` outside |
| SWAG `emerg` / every site 502 or 426 | A hand-written vhost re-sets a header or timeout from `proxy.conf`; remove it |
| UI 200 instead of 302 | `auth.enabled = false` or the path matches a too-broad `publicPaths` regex |
