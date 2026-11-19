# Access control and inherited defaults (plan)

Status: plan, not built. Two parts that share one mechanism:

1. **Inherited defaults.** One way for a setting to take its default from
   another setting (server name, domain, default access), visible the same
   way in the web UI everywhere.
2. **Access control.** A global default access (`Reachable from`, login) that
   every app follows unless it overrides it, plus an **Access** page that shows
   every entry point of the server: apps, open ports, and who can log in.

Part 2 builds on part 1. Build part 1 first.

---

## Part 1 — Inherited defaults

### Problem today

Three patterns exist for "default comes from somewhere else":

| Pattern | Example | What the UI shows | Where the value is resolved |
|---------|---------|-------------------|-----------------------------|
| A. `default = config.neo.…` | `backup.destinations.*.remotePath`, `storeClient` = `core.hostname`; `core.git.userEmail` = `neo@<hostname>` | The value as a placeholder. No hint where it comes from. | In the option |
| B. `nullOr` + `default = null` + "Empty uses …" in the text | `tailscale.hostname`, `netbird.hostname`, `netbird.managementUrl`, `monitoring-agent.name`, `gitea` web address | "Not set" | In `default.nix`, each module its own `if x != null then x else …` |
| C. `default = x or fallback` + `defaultText` | `netbird.networkRange` | The value | In the option |

So the operator cannot see which value a field will use, nor that it follows
another setting. Consumers repeat the null check.

The form already does the hard part: it saves only values that differ from the
default (`option_form.js`, "default" / "modified" state, reset per field), and
`PatchOp::Unset` removes a key. A missing key already means "follow".

### Design

One helper declares the link, and the default is the real value:

```nix
tailscale.hostname = lib.neo.mkInheritedOption {
  type = types.str;
  from = {
    path = ["core" "hostname"];        # settings path, for the UI link
    value = config.neo.core.hostname;  # or a function of it: "neo@${…}"
    label = "Server name";             # optional; the UI can read the source's ui.label
  };
  description = "Name of this server in Tailscale.";
  ui = …;
};
```

- `default` = `from.value`. Consumers read `cfg.hostname` directly. No `nullOr`,
  no null checks in `default.nix`.
- The option carries `inherits = { path; label; }` metadata. The extractor
  (`extract_service_options.nix`) passes it to the UI.
- A key in `settings.toml` overrides. Removing the key follows again.
- **Rule:** a value equal to the source is not saved (the form already drops
  values at their default). So "same as the default" means "follow it". This is
  the right behaviour for nearly every case. No separate "pin" flag.

#### UI (one rendering for every type)

- At default: the field shows the inherited value dimmed, with a chip
  **"From Server name"** that links to the source field (`option:` link).
  For lists and checkboxes (ingress) the controls show the inherited value
  with the same chip.
- Changed: a **"Changed here"** chip and the existing per-field reset, labelled
  **"Use Server name"** instead of a plain reset.
- The source field shows **"Used by N settings"** (optional, phase 1b), from a
  reverse index the extractor builds.

#### Values that are not known at evaluation

Some defaults are decided at runtime or mean "off", not "another setting"
(empty password = no password page, `buildCores` empty = Nix decides). These
stay `nullOr` but declare `ui.emptyMeans = "Nix decides"`. The UI shows that
text as the placeholder instead of "Not set". The description keeps the
sentence. This replaces free-text "Empty uses …" for anything the UI renders.

#### Enforcement

- Flake check `inherited-defaults`:
  - fails on an option `default` that reads `config.neo.*` without
    `mkInheritedOption` (detect by evaluating options: `default` differs when
    the source changes, or simpler: a `defaultText` / `inherits` must exist
    when `default` is not a literal);
  - fails on "Empty uses" / "Empty means" in a description of a `nullOr` option
    without `ui.emptyMeans`.
- Docs: `docs/LIB.md` (helper), `descriptions-and-guides.md` (texts),
  `web-ui-options.md` (chip), the `neo-core` / `neo-plugin` skills.

### Conversion list (from a grep, complete it during the work)

| Option | Today | After |
|--------|-------|-------|
| `core.git.userEmail` | A | inherits `core.hostname` (`neo@<name>`) |
| `backup.destinations.*.remotePath`, `.storeClient` | A | inherits `core.hostname` |
| `tailscale.hostname`, `netbird.hostname`, `monitoring-agent.name` | B | inherits `core.hostname` |
| `netbird.managementUrl` | B | inherits the NetBird server URL, or the netbird.io URL |
| `netbird.networkRange` | C | inherits `netbird-server.networkRange` |
| `gitea` web address, SSH clone address | B | inherits the app's public URL |
| `core.nix.maxJobs` / `buildCores`, password fields | B | `ui.emptyMeans` |
| `neo.monitoring.*.enabled` (internal) | A | no change (internal, not in the UI) |

Migration: none needed. Existing keys stay overrides. Optional: a settings
migration removes keys whose value equals today's source (for example
`remotePath = "<hostname>"`), so they follow from now on. Ask before doing it.

---

## Part 2 — Access control

### Global default access

New System section **Access** (`neo.core.access`):

| Option | Type | Default | Note |
|--------|------|---------|------|
| `defaultIngress` | list of `local` `tailscale` `netbird` `web` | all four (today's behaviour) | label "Apps are reachable from" |
| `defaultLogin` | bool | `true` | label "Apps ask for login", only for apps where login is available |

`mkReverseProxyOptions` changes `ingress` and `auth.enabled` to
`mkInheritedOption` with these as source. Every app and every plugin follows
with no change in its own module. Per app override = the existing field, now
with the "From default access" chip. `settings.toml` entries such as
`ingress = ["local"]` stay overrides.

An app that must never follow (for example SWAG's own dashboard, the
backup store, Let's Encrypt) passes a fixed default in
`mkReverseProxyOptions { ingress = …; }` (new argument; then it does not inherit).

Places that are not set up are shown, not removed: `tailscale` ticked while
Tailscale is off has no effect. The field marks it "Tailscale is off".

### Exposure model (data for the overview)

The page must be complete, so the data comes from the evaluated config, and a
check fails when something is open and not described.

New internal option `neo.core.exposure` (read-only for operators):

- **Apps** — generated from `getProxiedServices`: hosts (subdomain +
  `customDomains`), effective ingress, login (tinyauth on/off, `publicPaths`,
  tinyauth users/groups that may open it, OIDC client yes/no), and the app's
  own login when it declares one (`mkServiceMeta { ownLogin = "…"; }`).
- **Ports** — every port open to the outside: NixOS firewall
  (`allowedTCPPorts`, `allowedUDPPorts`, per-interface rules) and Docker
  published `ports` (these skip the NixOS firewall). Each entry:
  `{ port; protocol; service; label; interfaces; auth; }`.
  Modules declare them with `lib.neo.mkExposedPort`, which opens the port and
  writes the entry in one place.
- **Logins** — who can get in, per entry point:
  - SSH: users (admin, homeserver), authorized keys (comment + fingerprint,
    never the full key in logs), password login on/off, root login off,
    NetBird SSH on/off.
  - Console password set or not (`hashedLinuxPassword`).
  - tinyauth: users, groups, OAuth providers.
  - Per app: `auth` kind `tinyauth` | `oidc` | `app` | `none`, and the text
    for `app` ("Gitea accounts", "Registry users", "Pi-hole password").

Flake check `exposure-complete`: every firewall port and every Docker
published port has an exposure entry. This keeps the page honest as services
are added.

### Access page (web UI)

New top-level page **Access**, eval-backed (one `QueryKey`, the evaluation
pool, `data-neo-eval-*` like other pages).

1. **Default access** — the two global settings, editable inline.
2. **Apps** — one row per app, a column per place (Home network, Tailscale,
   NetBird, Internet). Cell: allowed / blocked / place not set up. Badges:
   "follows default" or "changed". A Login column. Click a row → the app's
   access settings.
3. **Other entry points** — ports table: port, protocol, service, from where,
   login (SSH → link to section 4).
4. **Logins** — SSH keys, console password, tinyauth users and providers.
5. **Warnings** at the top, same data: Internet + no login, an open port with
   no login (registry with no users, Pi-hole with no password), SSH password
   login on because no key is set, `web` ticked but no public address.

Phase 2 of the page: tick and untick cells in the matrix. Each change is a
`PatchOp` through `apply_edit` and takes the settings lock, like the form.
Ticking the same as the default unsets the key.

Hermes: a `neo-access` skill (or part of the SWAG skill) reads the same JSON,
so the assistant can answer "what is open to the internet?".

### Later (not in this plan): enforce ingress on ports

Today `ingress` acts only in SWAG (HTTP). Ports such as SSH, Syncthing,
Gitea SSH are open on all interfaces. A later step can give each exposed port
an `ingress` too and turn it into nftables rules per interface (LAN interface,
`tailscale0`, `wt0`, public). The exposure model above is the base for it.
Decide after the overview shows how many such ports exist.

---

## Phases

| # | Work | Acceptance |
|---|------|-----------|
| 1 | `mkInheritedOption`, `inherits` metadata in the extractor, UI chip and "Use …" reset, `ui.emptyMeans` | Widget tests; `just launch`; tailscale/netbird names show "From Server name" |
| 2 | Convert the list above; flake check `inherited-defaults`; docs and skills | `just check` green; no "Empty uses" left without `emptyMeans` |
| 3 | `neo.core.access.*`; `ingress` and `auth.enabled` inherit; SWAG maps unchanged | Change default → every app without an override follows (VM: 404 from a blocked place) |
| 4 | `neo.core.exposure`, `mkExposedPort`, check `exposure-complete` | Check fails on a raw `allowedTCPPorts` |
| 5 | Access page, read only, with warnings | VM: page lists SSH 22, 80/443, Pi-hole 53, … with the right login |
| 6 | Inline edit in the matrix; Hermes skill | Edit writes a `PatchOp`; same-as-default unsets |

## Open questions

1. `defaultLogin`: is a global login default wanted, or only `defaultIngress`?
2. Migration: remove keys that equal today's source (so they follow), or keep
   them as overrides?
3. Ports enforcement (the "Later" section): in scope now or later?
4. The Access page: its own top-level page, or a tab of the System page?
