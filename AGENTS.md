# AGENTS.md — Neo development guide

Map for humans and coding agents. Deeper docs: [README.md](README.md), [docs/INSTALL.md](docs/INSTALL.md), [docs/RESTORE.md](docs/RESTORE.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/CLI.md](docs/CLI.md), [docs/PLUGINS.md](docs/PLUGINS.md).

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
| `just web` | Web UI on the host against the existing `build/` (run `just build` once first) |
| `just test-widgets` | JS tests for the web UI widgets and static scripts |
| `just exec 'CMD'` / `just logs SVC` / `just ssh` | Guest shell / `journalctl -b -u SVC` |
| `just test-disko-boot` | Install the Disko layout into disk images and boot them from disk under UEFI and BIOS (GRUB, firmware boot entry). `just launch` boots the kernel directly and never tests the boot loader |
| `just format` / `just check` | alejandra + cargo fmt / flake check. Tracked pre-commit in `.githooks` runs `just format`; `nix develop` sets `core.hooksPath` (skip with `git commit --no-verify`) |

**Flakes only see git-tracked files** — `git add` new `nix/` paths before `just launch`.

## Add a service

1. `option.nix` — `enabled`, `// lib.neo.mkReverseProxyOptions`, `mkContainerDefinitions`, `mkAppdata`, `mkServiceMeta`, `mkSkillOptions`
2. `default.nix` — `mkIf`, activation dirs, `oci-containers` on `networks = ["internal"]`
3. `swag.nix` if public; `skills.nix` for Hermes (`mkServiceSkill`)
4. Enable in **`settings.toml`**: `[services.<name>] enabled = true`
5. `git add` → **`just launch`** → smoke test below → `just format && just check`

Naming: options camelCase under `neo.services.*`; plain-string descriptions; volumes via `config.neo.core.volumes.*`.

**Option texts** (read by non-technical users in the web UI):

- `description`: first sentence says what the setting does for you, in plain words (≤ ~20 words). Optional second sentence: the consequence or a technical detail ("Changing it signs everyone out."). Say what empty / the default means.
- No env var names, file paths, unit names or `services.x.y` paths in descriptions; put them in a Nix comment. Refer to other settings by their label.
- `enabled`: say what you get ("Private web search that doesn't track you"), not "X service". `lib.neo.mkEnableOption` uses the text as the whole description (capitalised, full stop added), without the "Whether to enable" prefix.
- Give every non-obvious key a `ui.label` ("Skip Beszel login", not `enableSingleUserSystem`).
- Secrets (passwords, keys, tokens) always stay visible, never in Advanced: the operator sets each one (usually with Generate) and should know it exists. Say what it protects and what changing it later breaks.
- Ports and other plumbing go in `ui.group = lib.neo.ui.groups.advanced`.
- Fields that only matter with another setting get `ui.visibleWhen` (a bool or a non-empty string hides the field while false/empty; `"opt=value"` for a choice).

**Service texts:** `mkServiceMeta` takes `tagline` (one line on the grid card, what you get: "Your own Google Photos"), `description` (2–3 plain sentences for non-technical users: what you can do, what it replaces), `notes` (operator facts shown collapsed as "Good to know": first login, paths that skip tinyauth, upgrade cautions), `replaces` (`[ "Google Photos" ]`) and `privacy` (privacy map category ids). No upstream marketing, env var names or internals in `tagline`/`description`. Without a `tagline` the card shows the first line of `description`. Rules and examples: `docs/design/descriptions-and-guides.md`.

**Service icons:** `mkServiceMeta.icon` is a URL string or a file path (`./icon.svg`, png, webp, ico, …). Paths are copied into the Nix store and served at `/service-icons/…` in the services grid, navigator, and option pane. `https://…` and site paths such as `/static/neo-icon.png` stay as written.

**Units / status:** every unit a service runs goes in `systemdUnits` (`mkContainerDefinitions` + `extraUnits`, or `mkSystemdUnits`); the web UI status and `neo-<service>.target` read it. Post-start configuration (occ, config patches, provisioning) uses **`lib.neo.mkSetupService`** (`nix/lib/setup-service.nix`): retry loop, RemainAfterExit so success shows as "done", Type=simple unless dependents need `blocking = true`. Status health comes from systemd properties (`units/status.rs` `UnitHealth`), never unit names.

**Appdata:** all mutable service state (databases, uploads) lives under `${config.neo.core.volumes.appdata}/<name>` and is declared with `mkAppdata`, so Clear-appdata and snapshots cover it. This also applies to native NixOS modules (not just OCI): point their paths there (`database.path`, …), replace `DynamicUser`/`StateDirectory` with `User`/`Group = "homeserver"` (the neo uid/gid, like OCI appdata) plus `ReadWritePaths` and a tmpfiles `d` rule (see `nix/services/vikunja/default.nix`) — never leave data in `/var/lib/<name>`.

**Secrets:** declare passwords/tokens/keys with `lib.neo.mkSecretOption` (keep the type/default you would use, e.g. `nullOr str`). Never put a secret into a container/unit `environment`, `/etc` text or a script: use `neo.core.secrets.containerEnv.<container> = { VAR = …; }` (rendered env file) or `neo.core.secrets.templates."<name>" = { content; owner; mode; restartUnits; }` and its `.path` (`LoadCredential` for DynamicUser). The value is a placeholder on encrypted configs, so never hash/encode a secret at eval: use `lib.neo.secrets.sha256` / `base64` / `jsonString` (computed at activation), and `lib.neo.secrets.swagSnippet` for SWAG conf lines that carry a secret. For a secret inside JSON/JS use `lib.neo.secrets.jsonString`; a file a service expects at a fixed path takes `copies = [ path ]`. Operators switch storage (inline → `secrets.yaml` → encrypted `secrets.sops.yaml`) in Settings → Secrets or with `neo secrets split|enable|disable|rekey`; `neo restore <repo>` reinstalls from the repo with the escrowed host key. See `docs/design/secrets.md`, `nix/modules/core/secrets.nix`.

**SWAG traps:** `include /config/nginx/proxy.conf` already sets Upgrade/Connection and proxy timeouts — **do not re-set** them (426 WebSockets / `proxy_*_timeout` duplicate kills all vhosts).

**ingress:** per-service multi-select (`local` / `tailscale` / `web`) via `mkReverseProxyOptions`; default all three. Omitting `web` denies that vhost on the shared rathole/PROXY-protocol listener (neo 404 page) — not a per-app rathole port.

**tinyauth:** default edge auth. Health probes need `auth.publicPaths` (e.g. `^/api/v1/info/status$`). UI stays 302 → tinyauth.

**Hermes:** per-service `skills.nix` → `neo-<name>`; use `mkServiceSkill`; credentials = real settings keys only. Collector: `nix/services/hermes/skills.nix`.

**Git identity:** `neo.core.git` (`nix/modules/core/git.nix`) is the one machine git user: SSH key `/var/lib/neo/git/id_ed25519` (root:neo-git 0640), `tokens` (git credential helper + Nix `access-tokens`), `knownHosts`, `userName`/`userEmail` (system gitconfig, config repo commits). A module whose user needs git appends to the internal `neo.core.git.users`; never hand-roll per-service ssh/git config. Root is not a member (switch reuses fetched inputs).

**Rust:** clap, `anyhow::Result`, `?` + `.context`, `toml_edit::DocumentMut`; no `unwrap` in lib paths.

## Disko, boot, restore

`nix/modules/disko/`: the main disk is EF02 (1M) + ESP (`/boot`) + ZFS `zroot` (`root` → `/`, `neo` → `/var/neo`, snapshotted). GRUB: UEFI removable path from `core/base.nix`; `boot-install.sh` (grub `extraInstallCommands`) adds the BIOS install when the EF02 partition exists, a NixOS firmware entry and `startup.nsh`. It must never fail a switch: log and go on. Disks partitioned before EF02 existed stay UEFI-only, so never put the main disk into `boot.loader.grub.devices`. Boot changes need `just test-disko-boot`. Moving or restoring a machine: [docs/RESTORE.md](docs/RESTORE.md).

## Guides

Outcome-focused how-tos ("Block ads for the whole home") on the web UI's **Guides** tab, linked from every service they use. Declare one per directory, `nix/guides/<id>/default.nix` (core) or in a plugin:

```nix
flake.modules.nixos.guide-<id> = {
  neo.guides.<id> = {
    title = "…"; summary = "…";
    services = [ "pihole" ]; optional = [ "tailscale" ];
    difficulty = "easy"; minutes = 15; tags = [ "network" ]; rank = 10;
    body = ./guide.md;
  };
};
```

The body is GitHub-style Markdown (options in `nix/modules/core/guides.nix`, renderer `cli/src/commands/web/guides.rs`). Links: `[Pi-hole](service:pihole)`, `[home address](option:pihole.localIP)` (opens the field), `[…](guide:<id>)`, `[…](privacy:<category>)`; targets missing on this machine render as plain text. Placeholders `{{domain}}` and `{{service.<name>.url}}`. Callouts `> [!tip]` / `> [!warning]` / `> [!check] Title`; a fenced `neo-check` block inside a check is the command the assistant runs (shown only when Hermes is on). Raw HTML is shown as text. Write for non-technical readers, one outcome per guide, steps that point at real settings.

## Assistant chat and settings tools

With Hermes on, two visible secrets turn on the AI parts of the web UI (`nix/services/hermes/default.nix`):

- **Gateway token** (≥16 chars) → Hermes' API server (`API_SERVER_KEY`, 127.0.0.1:`gatewayPort`) and the **chat drawer** (`static/agent_chat.js`, `partials/agent_drawer.html.hbs`, routes `routes/agent.rs` → `agent.rs`). neo-web proxies `/agent/open|chat|approve|stop|render` to the session chat SSE API; the key stays server-side. Open it from any element with `data-neo-agent="general|service:<name>|guide:<id>"` (+ `data-neo-agent-title`, `-starter`, `-hint`); render such buttons only when the context has `assistant` (`agent::available()`). **`@` references** (`agent_refs.rs`): `GET /agent/refs` lists services, settings (`extract_option_index.nix`), guides and privacy categories (cached until settings.toml / flake.lock change); the drawer fuzzy-matches them client-side. Tokens `@svc`, `@svc.opt`, `@core.opt`, `@guide:<id>`, `@privacy:<id>` in a sent message get a `<neo-references>` block appended (secrets redacted like the settings tools), which `agent::history` strips again. Tool calls render as collapsible rows (input from `tool.started.args`, output from `run.completed.messages`, duration from event `ts`).
- **Settings tools token** → MCP server `neo` for Hermes at `/internal/mcp` (`routes/agent_tools.rs`; SWAG 404s `/internal/`): read settings (secrets redacted) and **propose** changes. A proposal (`proposals.rs`, card `proposal_card.html.hbs`, `/proposals/<id>`) writes nothing until the user clicks Save (`settings::edit::apply_edit`, the form's write path) or Save & apply (then `apply_or_activate`). New settings writes go through `apply_edit` too, never a second path.

**Privacy map** (`/configuration/privacy`): categories in `nix/modules/core/privacy.nix` (`neo.privacy.categories.<id>`, plugins may add some), services tag themselves with `mkServiceMeta { privacy = [ "calendar" ]; }` (unknown ids are a build warning), guides with `privacy = [ … ]`. A category counts as "at home" once a tagged service is enabled.

Every guide is also a Hermes skill `neo-guide-<id>` (`nix/services/hermes/skills.nix`), which "Do it with AI" loads. **Show guides** (`neo.services.neo.guides`, default on) hides the Guides tab, the privacy map, the guides on service pages and the guide skills (extracts read `guidesEnabled` from `extract_lib.nix`).

## Web UI option metadata (`rank` / `helper` / `ui`)

Declare presentation next to the option in Nix. Extract serializes it; the form interprets it generically — **do not** hardcode service/option names in `option_form.js` or `extract_service_options.nix`.

| Field | Role |
|-------|------|
| `rank` | Sibling sort order (see `nix/lib/option.nix`) |
| `helper` | Fill-assist (`lib.neo.helpers.*`, scripts under `nix/lib/helpers/`) |
| `ui` | Widgets, dynamic multi-select choices, key linkage, save prune (`nix/lib/ui.nix`) |

Recommended top-level **rank bands** for `neo.services.<name>` (siblings only; same table as `nix/lib/option.nix`):

| Rank | Option |
|------|--------|
| 0 | `enabled` |
| 10–89 | service-specific |
| 100 | `subdomain` (`mkReverseProxyOptions`) |
| 105 | `ingress` (`mkReverseProxyOptions`) |
| 110 | `vpn` (`mkVpnOptions`) |
| 120 | `auth` (`mkReverseProxyOptions`) |
| 130 | `customDomains` (`mkReverseProxyOptions`) |
| 200 | `skill` (`mkSkillOptions`) |
| 300 | `containers` (`mkContainerDefinitions`) |

### Sections and labels (`ui.group` / `label` / `summary` / `choiceLabels` / `visibleWhen`)

The service pane shows **General** (ungrouped options, always open) followed by collapsible **groups**. `ui.group = lib.neo.ui.groups.access` (or `mkGroup { id; label; description; icon; rank; }`) on an option puts it and all its descendants in that section; generated option sets already do this (`mkReverseProxyOptions`/`mkVpnOptions` → access, `mkSkillOptions` → assistant, `mkContainerDefinitions` → advanced). Section split + humanized labels happen in `cli/src/commands/web/nix/sections.rs`. Group `icon` takes a path or URL like `mkServiceMeta.icon`: built-ins are `lib.neo.ui.icons.<name>` (every `nix/lib/icons/<name>.svg`), and a plugin can pass its own `./icon.svg`. SVGs are drawn as a `currentColor` mask so they follow the theme; other images render as-is.

- `label` — human name (default: humanized last path segment)
- `summary = true` — value chip on the collapsed group header
- `choiceLabels = { web = "Internet"; }` — display names for enum / choice values
- `visibleWhen = "enabled"` — sibling bool; the field hides while it is false; `visibleWhen = "type=entra"` — hides unless the sibling equals that value (also inside attrsOf entries)

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
| `primaryItemList` | listOf scalars; first entry is the primary (badge from `entryLabel`, e.g. Hermes `telegramAllowedUserId` home channel) |
| `providerAuth` | submodule of provider + API key and/or OAuth login + model (Hermes `llm`). Catalog rows declare `hasApiKey` / `hasOauth` / `oauthFlow` / `needsBaseUrl`; child option descriptions render as ⓘ on each input; `ui.oauth.script` runs status/login/refresh |
| `proxyRouteList` | attrsOf str of public domain → plain `http://` upstream (SWAG `proxyPass`). Route cards `https://domain → TLS at SWAG → upstream` with add/remove, inline checks (hostname only, `http://host[:port]`, duplicates = error; `https://`, path, loopback = warning; no port = hint) and one-click fixes. Errors block save through the optional widget `validate(name) → string[]` hook |

Implementations: `cli/templates/options/widgets/<name>.html.hbs` + `<name>.js` (file names in snake_case, e.g. `proxy_route_list.js`) (registers on `NeoWidgets`) + `<name>.test.js`. `option_form.js` mixes in registered widgets and dispatches init/save/reset on `ui.widget` only; a widget may also define `validate(name)` returning error strings, and `save()` refuses (toast + error flash) while any remain. Dispatch in `attrs_of.html.hbs` / field templates on `ui.widget`, not on option names. Run `just test-widgets`.

### Adding a new special UI (checklist)

1. Prefer composing **choices** + **keysFrom** + **save** only; add a **widget** only if the generic type editor is not enough.
2. Declare `ui` on the option in `option.nix` via `lib.neo.ui.mkUi { ... }`.
3. If you need a new dynamic list, add a **choice provider** in extract (one attr, reused by any service).
4. If you need a new composite editor: one Handlebars partial + one JS module under `options/widgets/` that `NeoWidgets.register`s by `ui.widget` name, a colocated `*.test.js`, and a `<script>` in `configuration.html.hbs` after `registry.js` — no `if (name === "access")`.
5. Document the widget/provider in this section.

Reference consumers: `nix/services/tinyauth/option.nix` (`access` + `ui.choices = "authApps"`); `nix/services/hermes/option.nix` (`telegramAllowedUserId` + `primaryItemList`); `nix/services/swag/option.nix` (`proxyPass` + `proxyRouteList`).

### Loading feedback

`cli/static/nav_progress.js` hooks htmx events generically: any request swapping into `#config-content` gets a delayed top progress bar (`#neo-progress`), a pressed/spinner state on the clicked element (`data-neo-loading`), a veiled target (`aria-busy` + `.neo-swap-busy`) and a "Loading…" label after ~1.5s. Opt other requests in with `data-neo-progress` (on the element or an ancestor), out with `data-neo-progress="false"`. Do not add per-page loading hacks.

### Operation locks

Every mutating operation (web route, background job, CLI subcommand, updater timer) takes lock **scopes** from `cli/src/utils/locks.rs` (`LockSpec`): `system` exclusive for activate / update / generation switch / store repair / data restore; `system` shared + `service/<name>` + every `unit/<unit>` exclusive for snapshot restore and clear appdata; `unit/<unit>` for start/stop/restart and image pull; `service/<name>` shared for a snapshot; `system` shared for settings writes. Web routes use `web::locks::try_lock` (long jobs move the guard into their task), answer conflicts with `Blocked` (409, toasted by `static/locks.js`), and mark buttons with `data-neo-lock="<scope>[:sh] …"` (`lock_attr`), which `locks.js` disables while a live holder conflicts. Restores also set unit **start guards** (`/run/neo/guard/<unit>`, enforced by a global `AssertPathExists=!` drop-in in `nix/modules/core/operation-locks.nix`). New mutating operation: pick a `LockSpec`, take it, annotate its button — no ad-hoc in-progress checks.

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
