# AGENTS.md — Neo development guide

Map for humans and coding agents. Why things are this way: [docs/PHILOSOPHY.md](docs/PHILOSOPHY.md). Deeper docs: [README.md](README.md), [docs/INSTALL.md](docs/INSTALL.md), [docs/RESTORE.md](docs/RESTORE.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/CLI.md](docs/CLI.md), [docs/PLUGINS.md](docs/PLUGINS.md), [docs/PLUGIN-DEVELOPMENT.md](docs/PLUGIN-DEVELOPMENT.md), [docs/LIB.md](docs/LIB.md). Designs per area: [docs/design/](docs/design/) (read the one for the area you change).

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
| `just exec 'CMD'` / `just logs SVC` / `just ssh` | Guest shell / `journalctl -b -u SVC` |
| `just test` | Rust tests (`cargo test`) and `test-widgets` |
| `just test-widgets` | JS tests for the web UI widgets and static scripts |
| `just clippy` | `cargo clippy --all-targets -D warnings` |
| `just format` | alejandra + cargo fmt. Tracked pre-commit in `.githooks` runs it; `nix develop` sets `core.hooksPath` (skip with `git commit --no-verify`) |
| `just check` | Formatting check, then `nix flake check` (Nix checks, Rust tests + clippy, widget tests). Lists untracked files the flake cannot see; does not `git add` them |
| `just test-disko-boot` | Install the Disko layout into disk images and boot them from disk under UEFI and BIOS (GRUB, firmware boot entry). Needs KVM; minutes. `just launch` boots the kernel directly and never tests the boot loader |

**Flakes only see git-tracked files** — `git add` new `nix/` paths before `just launch`.

## Add a service

1. `option.nix` — `enabled`, `// lib.neo.mkReverseProxyOptions`, `mkContainerDefinitions`, `mkAppdata`, `mkServiceMeta`, `mkSkillOptions`
2. `default.nix` — `mkIf`, appdata dirs (`neo.core.containerDirs.<container>`), `oci-containers` (each container of the service is on `app-<name>` by default, also with a link or host access)
3. `swag.nix` if public; `skills.nix` for Hermes (`mkServiceSkill`); `monitoring.nix` for metrics and a Grafana dashboard (`neo.monitoring.apps.<name>`, `lib.neo.grafana`, see Monitoring)
4. Enable in **`settings.toml`**: `[services.<name>] enabled = true`
5. `git add` → **`just launch`** → smoke test below → `just format && just check`

Naming: options camelCase under `neo.services.*`; plain-string descriptions; volumes via `config.neo.core.volumes.*`.

### Defaults from other settings

A default that is another setting, or computed from it (server name, domain, the app's own subdomain), is a `lib.neo.mkInheritedOption { from = "core.hostname"; default = …; }`, never `nullOr` + "Empty uses …" + a null check in `default.nix`. An empty value that means "off" or "the program decides" stays `nullOr` with `ui.emptyMeans`. Check `inherited-defaults` (sentinel server name and domain). Design: [docs/design/access-and-defaults.md](docs/design/access-and-defaults.md).

### Option texts

Read by non-technical users in the web UI. Rules and examples: [docs/design/descriptions-and-guides.md](docs/design/descriptions-and-guides.md).

- `description`: first sentence says what the setting does for you, in plain words (≤ ~20 words). Optional second sentence: the consequence or a technical detail ("Changing it signs everyone out."). Say what empty / the default means.
- No env var names, file paths, unit names or `services.x.y` paths in descriptions; put them in a Nix comment. Refer to other settings by their label.
- `enabled`: say what you get ("Private web search that doesn't track you"), not "X service". `lib.neo.mkEnableOption` uses the text as the whole description (capitalised, full stop added), without the "Whether to enable" prefix.
- Give every non-obvious key a `ui.label` ("Skip Beszel login", not `enableSingleUserSystem`).
- Secrets (passwords, keys, tokens) always stay visible, never in Advanced: the operator sets each one (usually with Generate) and should know it exists. Say what it protects and what changing it later breaks.
- Ports and other plumbing go in `ui.group = lib.neo.ui.groups.advanced`.
- Fields that only matter with another setting get `ui.visibleWhen` (a bool or a non-empty string hides the field while false/empty; `"opt=value"` for a choice, `"opt=a,b"` for one of several).
- Flake check `service-texts` fails on env var names, system paths and `services.x.y` paths in option descriptions, taglines and service descriptions, and on a service without `license`.

### Service texts and icons (`mkServiceMeta`)

Fields and types: [docs/LIB.md](docs/LIB.md) (`mkServiceMeta`); writing rules: [descriptions-and-guides.md](docs/design/descriptions-and-guides.md).

- `tagline`: one line on the grid card, what you get ("Your own Google Photos"). `description`: 2–3 plain sentences for non-technical users (what you can do, what it replaces). No upstream marketing, env var names or internals in either.
- Operator facts (first login, paths that skip tinyauth, upgrade cautions) go in `notes` ("Good to know").
- Every service declares `license` (`{ spdx; url; }`; `openSource = false` + `note` when closed code ships in the image or it depends on a closed service).
- `icon`: a URL or a file path (`./icon.svg`, png, …); paths are served at `/service-icons/…`, `https://…` and site paths such as `/static/neo-icon.png` stay as written.

### Units, scripts, appdata

- **Units / status:** every unit a service runs goes in `systemdUnits` (`mkContainerDefinitions` + `extraUnits`, or `mkSystemdUnits`); the web UI status and `neo-<service>.target` read it. Status health comes from systemd properties (`units/status.rs` `UnitHealth`), never unit names.
- **Setup units:** post-start configuration (occ, config patches, provisioning) uses **`lib.neo.mkSetupService`** (`nix/lib/setup-service.nix`): retry loop, RemainAfterExit so success shows as "done", Type=simple unless dependents need `blocking = true`.
- **Shell scripts:** more than a few lines go in a `<name>.sh` next to the module, built with `pkgs.writeShellApplication` (`errexit nounset pipefail`, shellcheck at build) and fed through `runtimeEnv` / `runtimeInputs`, never spliced into a `script` string (example: `nix/services/docker-updater`). For a setup unit pass `script = lib.getExe app`. Flake check `shellcheck` covers every `.sh` in `nix/` and `templates/`.
- **Appdata:** all mutable service state (databases, uploads) lives under `${config.neo.core.volumes.appdata}/<name>`, declared with `mkAppdata`, so Clear appdata and snapshots cover it. Native NixOS modules too: point their paths there (`database.path`, …), replace `DynamicUser`/`StateDirectory` with `User`/`Group = "homeserver"` (the neo uid/gid, like OCI appdata) plus `ReadWritePaths` and a tmpfiles `d` rule (see `nix/services/vikunja/default.nix`). Never leave data in `/var/lib/<name>`.

### Secrets

Design (storage inline → `secrets.yaml` → `secrets.sops.yaml`, `neo secrets …`, restore with the escrowed host key): [docs/design/secrets.md](docs/design/secrets.md), code `nix/modules/core/secrets.nix`.

- Declare passwords/tokens/keys with `lib.neo.mkSecretOption` (keep the type/default you would use, e.g. `nullOr str`).
- Never put a secret into a container/unit `environment`, `/etc` text or a script. Use `neo.core.secrets.containerEnv.<container> = { VAR = …; }` (rendered env file) or `neo.core.secrets.templates."<name>" = { content; owner; mode; restartUnits; }` and its `.path` (`LoadCredential` for DynamicUser). A file a service expects at a fixed path takes `copies = [ path ]`.
- The value is a placeholder on encrypted configs, so never hash/encode a secret at eval: use `lib.neo.secrets.sha256` / `base64` / `jsonString` (computed at activation; `jsonString` for a secret inside JSON/JS), and `lib.neo.secrets.swagSnippet` for SWAG conf lines that carry a secret.
- Flake check `secret-leak` sets every secret setting to a sentinel and fails when one reaches an environment, a unit, `/etc` or an activation script.

### Reverse proxy, login, networks

- **SWAG trap:** `include /config/nginx/proxy.conf` already sets Upgrade/Connection and proxy timeouts — **do not re-set** them (426 WebSockets / `proxy_*_timeout` duplicate kills all vhosts).
- **ingress** (`mkReverseProxyOptions`): `local` / `tailscale` / `netbird` / `web`, default all four. Omitting `web` denies that vhost on the shared rathole/PROXY-protocol listener (neo 404 page), not a per-app rathole port. Classification: `netbird` (`services.netbird.networkRange`) wins over LAN and Tailscale; the PROXY-protocol port and public IPv6 (SWAG on the IPv6-only `swag-ipv6` network) are `web` ([netbird.md](docs/design/netbird.md)).
- **Default access** (`neo.access`, nix/modules/core/access.nix): every app's `ingress`, `auth.enabled` and `sso` (`lib.neo.mkSsoOption`) follow `defaultIngress` / `defaultLogin` / `defaultSso` until set for the app; pass `ingress` / `auth.enabled` to `mkReverseProxyOptions` only when the app must not follow. Declare the app's own accounts with `mkServiceMeta { ownLogin = "…"; }`. The Access page (Settings → Access) edits all of it.
- **Open ports:** every firewall port and published Docker port (not on loopback) needs `neo.exposure.ports.<id> = { port; protocol; label; service; login; open; }` in the module that opens it (check `exposure-complete`).
- **tinyauth** is the default edge auth. Health probes need `auth.publicPaths` (e.g. `^/api/v1/info/status$`); the UI stays 302 → tinyauth.
- **OIDC:** an app that can log in with OIDC registers itself inside its `mkIf` as `neo.services.tinyauth.oidcClients.<id> = { name; redirectUris; }` and reads `.clientId` / `.clientSecret` (derived from a machine-generated key, no setting; pass it through `containerEnv`) and `tinyauth.oidcIssuer` (example: `nix/services/vaultwarden`, option `sso`). Users from OAuth providers carry their groups in the `groups` claim.
- **Docker networks** ([docs/design/network-isolation.md](docs/design/network-isolation.md)):
  - Each service has its own network `app-<service>` (`lib.neo.docker.serviceNetwork`), the default for every container in its `systemdUnits`. Containers of different services cannot reach each other (flake check `docker-network-isolation`).
  - A service that must call another one inside Docker declares a link (`lib.neo.docker.mkLink { name; containers; }`, e.g. Nextcloud ↔ Collabora); otherwise it uses the other app's public URL. A container with a link or `mkHostAccess` stays on its service network (Neo adds it first, also for a link a plugin adds to a core container).
  - Host services (neo web, Hermes, Vikunja) are reachable only from the `neo-host` bridge: SWAG, and containers with `lib.neo.docker.mkHostAccess`.
  - SWAG joins every proxied service's network; gluetun stands in for VPN-routed containers on all their networks, aliases included (check `vpn-networks`). `internal` stays for plugins only; `neo-docker-network` creates every named network and removes empty undeclared ones.

### Restart-only changes

A change that only a server restart applies raises its component's own trigger inside its `mkIf cfg.enabled`: `neo.core.restart.triggers.<service> = { level; reason; }` (core: `triggers.core` in `nix/modules/core/restart.nix`; plugins: `plugin.name "<svc>"`; `onEnable` when even switching it on needs one). Kernel and Docker settings need no trigger. The updater's restart logic runs from the *old* system, so a change to it takes effect one run late. Checks: `restart-required`, `restart-check`. Details: [docs/design/restart-required.md](docs/design/restart-required.md).

### Monitoring

- `monitoring` (Grafana, Prometheus, Loki) on one machine, `monitoring-agent` (Alloy) on every machine.
- Apps declare Prometheus endpoints, dashboards and alert rules in `neo.monitoring.apps.<service>` (`metrics`, `dashboards`, `alerts`; `nix/modules/core/monitoring.nix`, example `nix/services/immich/monitoring.nix`), never by hand in Grafana. Dashboards and alert rules Neo or apps provide are read-only; what users or Hermes create is theirs.
- Exporters are switched on with `config.neo.monitoring.collecting`; an endpoint that needs a credential uses `auth` and the app's generated `scrapeToken` (no setting); metrics on the public port go in `hidePaths` (SWAG 404).
- Standard app telemetry (JSON logs, `neo_app_*` metrics, `lib.neo.grafana.app.*` sections, `lib.neo.monitoring.appAlerts`; helpers in `templates/plugin/telemetry/`): [docs/APP-TELEMETRY.md](docs/APP-TELEMETRY.md). Design: [docs/design/monitoring.md](docs/design/monitoring.md).

### Hermes, git identity, Rust

- **Hermes skills:** per-service `skills.nix` → `neo-<name>`; use `mkServiceSkill`; credentials = real settings keys only. Collector: `nix/services/hermes/skills.nix`.
- **Herdr:** `hermes.herdr` (`nix/services/hermes/herdr.nix`) lets the Herdr desktop app start Hermes over SSH as admin: linger for admin's hc core, and a `hermes` shim in admin's PATH that sudo-runs the CLI as the hermes user (never a second HERMES_HOME). hc itself comes from `hc machine add --install`, not from Neo.
- **Git identity:** `neo.core.git` (`nix/modules/core/git.nix`) is the one machine git user: SSH key `/var/lib/neo/git/id_ed25519` (root:neo-git 0640), `tokens` (git credential helper + Nix `access-tokens`), `knownHosts`, `userName`/`userEmail` (system gitconfig, config repo commits). A module whose user needs git appends to the internal `neo.core.git.users`; never hand-roll per-service ssh/git config. Root is not a member (switch reuses fetched inputs).
- **Rust:** clap, `anyhow::Result`, `?` + `.context`, `toml_edit::DocumentMut`; no `unwrap` in lib paths.

## Agent skills

`agent/` is a Claude Code plugin (marketplace `.claude-plugin/marketplace.json`) with the skills `neo-plugin` and `neo-core` (`agent/skills/*/SKILL.md`). They repeat the rules that matter most and point at the docs: when you change a rule, helper signature, check or error message they mention, update them in the same commit. `templates/plugin/AGENTS.md` is the short version shipped into every new plugin.

## Plugins

Design: [docs/design/plugin-instances.md](docs/design/plugin-instances.md).

- `core.plugins.<id> = { url; prefix; }` (old configs: a list of URLs, ids from the repo name; migration `014-plugin-tables`). Each id is the flake input `plugin-<id>`, so editing a URL touches one input.
- Plugins import `inputs.neo.flakeModules.plugin` and define `neoPlugin.modules.<name>`; each module gets a `plugin` argument and names everything with `plugin.name "<service>"`, so an instance with `prefix = "dev"` yields `dev-<service>`. Plugins without `neoPlugin` load once via `nixosModules.default`.
- Ownership (inventory, removal) keys on the id: declarations are tagged `neo-plugin:<id>#…`.
- The id/label rules exist in Nix (`templates/homeserver/modules/_plugin-ids.nix`, imported by `nix/lib/plugin.nix`), Rust (`cli/src/utils/plugins.rs`) and the widget JS, and must stay equal.

## Disko, boot, backup, restore

- **Disk layout** (`nix/modules/disko/`): EF02 (1M) + ESP (`/boot`) + ZFS `zroot` (`root` → `/`, `neo` → `/var/neo`, snapshotted).
- **GRUB:** UEFI removable path from `core/base.nix`; `boot-install.sh` (grub `extraInstallCommands`) adds the BIOS install when the EF02 partition exists, a NixOS firmware entry and `startup.nsh`. It must never fail a switch: log and go on. Disks partitioned before EF02 existed stay UEFI-only, so never put the main disk into `boot.loader.grub.devices`. Boot changes need `just test-disko-boot`.
- **`neo bootstrap`** (`cli/src/commands/bootstrap/`) is a wizard in front of `neo init` (`init_repo`) and `neo restore --data none` (`restore::remote::run`); it edits the streamproxy edge with `neo settings patch` ([bootstrap.md](docs/design/bootstrap.md)). Install changes belong in restore, not in bootstrap.
- **Backup and restore share one destination model** (`services.backup.destinations.<name>`, `target` ssh|disk|store × `format` files|encrypted|zfs): backup in `nix/services/backup` (`backup.sh`), `neo restore` in `cli/src/commands/restore/` (`local/` on the installer, `remote.rs` = nixos-anywhere kexec + run local there), settings read by `cli/src/utils/backup/` (shared with `neo backup`). **Change both sides together.**
- **Backup store** (`nix/services/backup-store`): each client key is forced to `neo-store-gate` (`cli/src/bin/neo-store-gate.rs`), an allowlist of what `backup.sh` and `neo restore` send, so **a new remote command in either needs a gate rule**. Storage stays outside `volumes.root`, by assertion.
- Design (priorities, `encrypted` = restic + `neo-backup.age`, key rotation, the WebSocket tunnel): [docs/design/backup-restore.md](docs/design/backup-restore.md); runbook: [docs/RESTORE.md](docs/RESTORE.md).

## Guides and privacy map

Outcome-focused how-tos ("Block ads for the whole home") on the web UI's **Guides** tab, linked from every service they use. One per directory, `nix/guides/<id>/default.nix` (core) or in a plugin, declaring `neo.guides.<id>` (`title`, `summary`, `services`, `optional`, `difficulty`, `minutes`, `tags`, `rank`, `body = ./guide.md`; example `nix/guides/adblock-at-home/`).

- Write for non-technical readers, one outcome per guide, steps that point at real settings.
- Links: `[Pi-hole](service:pihole)`, `[home address](option:pihole.localIP)` (opens the field), `[…](guide:<id>)`, `[…](privacy:<category>)`. Placeholders `{{domain}}`, `{{service.<name>.url}}`. Callouts `> [!tip]` / `> [!warning]` / `> [!check] Title`; a fenced `neo-check` block inside a check is the command the assistant runs.
- Every guide is also a Hermes skill `neo-guide-<id>`, which "Do it with AI" loads.
- Privacy map: categories in `nix/modules/core/privacy.nix` (`neo.privacy.categories.<id>`, plugins may add some); services tag themselves with `mkServiceMeta { privacy = [ … ]; }`, guides with `privacy = [ … ]` (unknown ids are a build warning).
- Markdown rules, rendering, the **Show guides** switch and the privacy map: [docs/design/descriptions-and-guides.md](docs/design/descriptions-and-guides.md).

## Assistant chat and settings tools

With Hermes on, the **Gateway token** turns on the chat drawer and the **Settings tools token** the MCP server through which Hermes proposes settings changes. Rules:

- One set of chat routes (`cli/src/commands/web/chat/`, `chat::routes()`): a new page with the drawer adds a `ChatHost`, never a second set of routes.
- Render `data-neo-agent` buttons only when `agent::available()` (the context has `assistant`).
- Settings writes go through `settings::edit::apply_edit` only (form, proposals, CLI). A proposal writes nothing until the user clicks Save.
- Secret values never pass through the agent (`generate` / `ask` / `copy` / `move` instead); everything shown to it goes through `web/redact.rs`.

Internals (routes, chats, `@` references, proposals): [docs/design/assistant-chat.md](docs/design/assistant-chat.md).

## Web UI option metadata

Declare presentation next to the option in Nix: `rank` (sibling order, `nix/lib/option.nix`), `helper` (fill-assist, `lib.neo.helpers.*`), `ui` (`lib.neo.ui.mkUi`: group, label, `visibleWhen`, `choices`, `keysFrom`, `widget`; `nix/lib/ui.nix`). **Never** hardcode service/option names in `option_form.js` or `extract_service_options.nix`: a new need becomes a choice provider or a widget usable by every service.

Rank bands for the top level of `neo.services.<name>` (siblings only):

| Rank | Option |
|------|--------|
| 0 | `enabled` |
| 10–89 | service-specific |
| 100 / 105 / 120 / 130 | `subdomain` / `ingress` / `auth` / `customDomains` (`mkReverseProxyOptions`) |
| 125 | `sso` (`mkSsoOption`; the app's SSO extras 126–129) |
| 110 | `vpn` (`mkVpnOptions`) |
| 200 | `skill` (`mkSkillOptions`) |
| 300 | `containers` (`mkContainerDefinitions`) |

- Widgets: `cli/templates/options/widgets/<name>.{html.hbs,js,test.js}` (snake_case), registered on `NeoWidgets`, dispatched on `ui.widget` only; run `just test-widgets`.
- Shared front-end pieces: `static/neo_util.js` (`NeoUtil`), `options/entry_field` and the `partials/`. Markup from Rust uses `data-neo-*` hooks, never inline `onclick`. Never `window.confirm` / `alert` / `prompt` (agents driving the browser cannot click them): `hx-confirm` and `NeoUtil.confirm` / `alert` / `prompt` show an in-page dialog.
- Loading feedback is generic (`cli/static/nav_progress.js`, `data-neo-progress`); no per-page loading hacks.

Sections, choices, `keysFrom`, the widget list, the checklist for a new special UI and the shared scripts: [docs/design/web-ui-options.md](docs/design/web-ui-options.md).

## Evaluations and operation locks

- **Evaluations:** every eval-backed page reads the evaluation pool (`config.eval.*`, `cli/src/commands/web/nix/pool/`), never a repl directly. A new eval-backed fragment root needs `data-neo-eval-keys`, `-gen`, `-provisional`, `-url`; a new query needs a `QueryKey`. Design: [docs/design/eval-latency.md](docs/design/eval-latency.md).
- **Operation locks:** every mutating operation (web route, job, CLI subcommand, timer) takes a `LockSpec` (`cli/src/utils/locks/`). Web routes use `web::locks::try_lock`, answer conflicts with `Blocked` (409) and mark buttons with `data-neo-lock` (`lock_attr`). No ad-hoc in-progress checks. Scopes and start guards: [docs/design/operation-locks.md](docs/design/operation-locks.md).

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
just exec 'docker exec <container> getent hosts <other-service-container> || echo isolated'  # must print isolated
```

**Pass:** unit active, internal OK, public health **200** (if bypassed), UI **302** to tinyauth, no SWAG `emerg`, other services' containers do not resolve. Iterate: fix → `git add` → `just launch`.

```bash
# Debug extras
just exec 'docker logs <name> 2>&1 | tail -80'
just exec 'docker logs swag 2>&1 | tail -40'
just exec 'docker exec swag cat /config/nginx/proxy-confs/<sub>.subdomain.conf'
```

## Don'ts

- No secrets in git; no force-push/commit unless asked; fix root causes.
- No sibling `imports` in dendritic modules; no hand-rolled sudo path triples (`mkSudoExtraRules`).
