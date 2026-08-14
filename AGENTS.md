# AGENTS.md — Neo development guide

Map for humans and coding agents. Deeper docs: [README.md](README.md), [docs/INSTALL.md](docs/INSTALL.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/CLI.md](docs/CLI.md), [docs/PLUGINS.md](docs/PLUGINS.md).

## What this is

NixOS homeserver flake (**flake-parts** + **import-tree** over `./nix`) + Rust **`neo` CLI**. Operators set `settings.toml` → `config.neo` → OCI services, SWAG, volumes. **Default validation: live QEMU smoke test** (`just launch` → guest checks → `https://<subdomain>.<domain>`).

| Area | Layout |
|------|--------|
| Services | `nix/services/<name>/{option,default,swag,skills}.nix` — auto-imported; **never** `imports = [ ./sibling.nix ]` |
| Lib | `nix/lib/` → `lib.neo` (proxy, containers, activation, skills, sudo, …) |
| Deploy template | `templates/homeserver` (`#homeserver`); plugins: `templates/plugin` |
| Operator config | `settings.toml` → `build/` via `neo init` (secrets: do not commit real ones) |
| CLI | `cli/` (clap); web UI under `commands/web/` |

## just (primary tools)

| Recipe | Role |
|--------|------|
| **`just launch`** | Default loop: shutdown → build → QEMU (SSH **:2222**, key `tools/development_ed25519`) |
| `just build` | `neo nuke` → `init` → `build` |
| `just status` / `shutdown` | VM health / stop |
| `just exec 'CMD'` / `just logs SVC` / `just ssh` | Guest shell / `journalctl -b -u SVC` |
| `just format` / `just check` | alejandra + cargo fmt / flake check |

**Flakes only see git-tracked files** — `git add` new `nix/` paths before `just launch`.

## Add a service

1. `option.nix` — `enabled`, `// lib.neo.mkReverseProxyOptions`, `mkContainerDefinitions`, `mkAppdata`, `mkServiceMeta`, `mkSkillOptions`
2. `default.nix` — `mkIf`, activation dirs, `oci-containers` on `networks = ["internal"]`
3. `swag.nix` if public; `skills.nix` for Hermes (`mkServiceSkill`)
4. Enable in **`settings.toml`**: `[services.<name>] enabled = true`
5. `git add` → **`just launch`** → smoke test below → `just format && just check`

Naming: options snake_case under `neo.services.*`; plain-string descriptions; volumes via `config.neo.core.volumes.*`.

**SWAG traps:** `include /config/nginx/proxy.conf` already sets Upgrade/Connection and proxy timeouts — **do not re-set** them (426 WebSockets / `proxy_*_timeout` duplicate kills all vhosts).

**tinyauth:** default edge auth. Health probes need `auth.publicPaths` (e.g. `^/api/v1/info/status$`). UI stays 302 → tinyauth.

**Hermes:** per-service `skills.nix` → `neo-<name>`; use `mkServiceSkill`; credentials = real settings keys only. Collector: `nix/services/hermes/skills.nix`.

**Rust:** clap, `anyhow::Result`, `?` + `.context`, `toml_edit::DocumentMut`; no `unwrap` in lib paths.

## Web UI option metadata (`rank` / `helper` / `ui`)

Declare presentation next to the option in Nix. Extract serializes it; the form interprets it generically — **do not** hardcode service/option names in `option_form.js` or `extract_service_options.nix`.

| Field | Role |
|-------|------|
| `rank` | Sibling sort order (see `nix/lib/option.nix`) |
| `helper` | Fill-assist (`lib.neo.helpers.*`, scripts under `nix/lib/helpers/`) |
| `ui` | Widgets, dynamic multi-select choices, key linkage, save prune (`nix/lib/ui.nix`) |

### `ui.choices` (multi-select)

On a `listOf str` (or nested field), set `ui.choices = "authApps"` (named provider) or `ui.choices = [ "a" "b" ]`. Extract attaches `type.values`; templates already render a checkbox grid when `type.values` is set.

Named providers live in `extract_service_options.nix` → `choiceProviders` (today: **`authApps`** = enabled services with reverse-proxy auth, excluding tinyauth). Add new providers there; never `if service == "…"`.

### `ui.keysFrom`

AttrsOf keys follow another option:

```nix
ui.keysFrom = lib.neo.ui.mkKeysFrom {
  option = "users";
  extract = "beforeColon";  # or "identity"
};
```

Extract prunes orphan keys from `current`; the form re-syncs when the source list/attrs change.

### `ui.widget` (composite editors)

| Widget | Use |
|--------|-----|
| `exclusiveListPair` | attrsOf submodule with exclusive list fields + open mode (e.g. tinyauth `access` allow/block) |
| `pluginList` | listOf flake URLs with add/remove cards and per-remove uninstall confirm (`core.plugins`) |

Implementations: `cli/templates/options/widgets/<name>.html.hbs` + `elp*` (or widget-prefixed) helpers in `option_form.js`. Dispatch in `attrs_of.html.hbs` / field templates on `ui.widget`, not on option names.

### Adding a new special UI (checklist)

1. Prefer composing **choices** + **keysFrom** + **save** only; add a **widget** only if the generic type editor is not enough.
2. Declare `ui` on the option in `option.nix` via `lib.neo.ui.mkUi { ... }`.
3. If you need a new dynamic list, add a **choice provider** in extract (one attr, reused by any service).
4. If you need a new composite editor: one Handlebars partial under `options/widgets/`, one init/save path in `option_form.js` keyed by `ui.widget` — no `if (name === "access")`.
5. Document the widget/provider in this section.

Reference consumer: `nix/services/tinyauth/option.nix` (`access` + `ui.choices = "authApps"` on allow/block).

## Live VM smoke test (default acceptance)

Domain = `services.swag.domain` in settings. OCI unit = `docker-<container>`.

```bash
just launch && just status
just exec 'echo up'                                          # wait until SSH works
just exec 'systemctl is-active docker-<name> docker-swag docker-tinyauth'
just logs docker-<name>
just exec 'docker exec swag curl -sS -m 10 http://<container>:<port>/…'   # internal
curl -sk -m 20 -w "\n%{http_code}\n" "https://<sub>.<domain>/health…"     # public (publicPaths)
curl -sk -m 20 -D - -o /dev/null "https://<sub>.<domain>/"                # expect 302 → tinyauth
```

**Pass:** unit active, internal OK, public health **200** (if bypassed), UI **302** to tinyauth, no SWAG `emerg`. Iterate: fix → `git add` → `just launch`.

```bash
# Debug extras
just exec 'docker logs <name> 2>&1 | tail -80'
just exec 'docker logs swag 2>&1 | tail -40'
just exec 'docker exec swag cat /config/nginx/proxy-confs/<sub>.subdomain.conf'
```

## Don'ts

- No secrets in git; no force-push/commit unless asked; fix root causes.
- No sibling `imports` in dendritic modules; no hand-rolled sudo path triples (`mkSudoExtraRules`).
