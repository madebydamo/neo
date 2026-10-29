# Descriptions, guides and the privacy map

Status: proposal · 2026-10-29

Neo's web UI is used by people who are not technical. Today the texts they see
are written for operators: upstream marketing, env var names, systemd syntax,
SWAG internals. This doc covers three things:

1. **Descriptions** – service and option texts that say what something does for you.
2. **Guides** – a knowledge base of nice setups ("ad blocking at home", "use your
   apps from anywhere with Tailscale"), with a *Do it with AI* button that lets
   the local Hermes walk you through it.
3. **Privacy map** – a picture of the data you spread over cloud services; click
   a tile (Calendar) to see the self-hosted alternative (RustiCal) and how to set
   it up.

All three are declared in Nix next to the services, so plugins can contribute,
links are checked at eval time, and Hermes gets the same content as the UI.

---

## 1. Service descriptions

### Problem

`mkServiceMeta.description` is used twice: as the 2-line card in the services
grid and as the pane header. The current texts mix three audiences:

- upstream pitch ("Changedetection.io is the best and simplest…", Hermes "$5 VPS")
- user value (often only in the first sentence, cut off on the card)
- operator notes: default credentials (mealie, dawarich), bypassed paths (dufs,
  rustical, calino, vikunja), pinned image tags (authentik, mealie, dawarich),
  internals (host.docker.internal, PROPFIND, AP_FRONTEND_URL)

### Proposal: split the meta fields

| Field | Shown in | Content |
|---|---|---|
| `tagline` | grid card, navigator tooltip, search | ≤ 80 chars. What you get. "Your own Google Photos" |
| `description` | pane header | 2–3 plain sentences: what you can do, what it replaces, who it is for |
| `notes` | collapsible "Good to know" in the pane | first login, which apps skip the login page, upgrade cautions, technical detail |
| `replaces` | search, privacy map | `[ "Google Photos" "iCloud Photos" ]` |
| `privacy` | privacy map | category ids, see §4: `[ "photos" ]` |

`tagline` falls back to the first sentence of `description` so plugins keep
working. `notes` is plain text (or a Markdown file path, rendered like guides).

### Writing rules

- Write for someone who knows what a calendar is, not what CalDAV is.
- Lead with the outcome ("Blocks ads on every device in your home"), not the
  technology ("DNS sinkhole").
- Name the familiar alternative when there is one ("like Google Photos").
- No superlatives, no upstream marketing, no version numbers.
- Operator facts go into `notes`, never into `tagline`/`description`.

Examples:

| Service | tagline | description |
|---|---|---|
| immich | Your own Google Photos | Phones back up photos and videos automatically. Find pictures by face, place or thing, and share albums with family. |
| pihole | Blocks ads for every device at home | Removes ads and trackers on phones, TVs and laptops without installing anything on them. Shows which devices talk to which sites. |
| rustical | Calendars and contacts that stay at home | Phones and computers sync with it like with Google or iCloud. Works with the built-in calendar apps on iPhone, Android, Mac and Windows. |
| tinyauth | The login page in front of your apps | Only you and the people you add can open your apps. Can sign in with Google or Microsoft. |
| docker-updater | Keeps your apps up to date | Checks for new app versions on a schedule and installs them. |

---

## 2. Option descriptions

### Findings (all 44 services, shared helpers, core)

No option lacks a description. The issues are quality and structure:

1. **Env var names lead the text** (14+ places): `AP_ENCRYPTION_KEY: …`,
   `PAPERLESS_SECRET_KEY: Django secret …`, `SECURITY_ENABLELOGIN`, `FIREFOX_CLI`.
2. **Internals in user text**: `services.swag.domain`, dnsmasq, OnSuccess hooks,
   Selkies, localPath/containerPath, "edge auth".
3. **Secrets read like plumbing**: database passwords and signing keys did not say
   what they protect or what changing them breaks. They stay visible (the
   operator sets each one, usually with Generate) — never hidden in Advanced.
4. **systemd calendar syntax** in backup, docker-updater, system-updater,
   ical-sync schedules.
5. **Almost no `ui.label`** outside tinyauth/swag/helpers: users see
   `enableSingleUserSystem`, `wireguardAddresses`, `garbageCollectOlderThan`.
6. **`enabled` texts are "X service"** (syncthing, searxng, pastebin "wantguns/bin").
7. **Missing `visibleWhen`**: `rebootAfterBoot` (on `activationMode=boot`),
   stirling `initialLogin*` (on `enableLogin`), authentik `email.*` (on `host`),
   gitea `ssh.*`, karakeep AI fields (on API key), hermes `llm.baseUrl`
   (provider = custom), tailscale `exitNodeAllowLanAccess` (on `exitNode`).
8. **Self-contradiction**: hermes `llm.model` says "leave empty" but defaults to
   a model; pihole `webPassword` says "Optional… do not leave it blank".

Worst offenders: `neo.iframeCookieSupport`, `hermes.superviseUpdates`,
`system-updater.rebootAfterBoot`/`activationMode`, `pihole.localIP`,
`tailscale.splitDns`, the whole `vpn` service (descriptions repeat the key).

The bar: tinyauth (groups, `visibleWhen`, `autoBan.ignoreIps` "Never ban"),
`registry.deleteEnabled`, `authentik.email.host` ("Empty: Authentik sends no
email"), `docmost.sso`.

### Rules (to add to AGENTS.md)

- First sentence: what the setting does for you, plain words, ≤ ~20 words.
- Optional second sentence: consequence or technical detail ("Changing it signs
  everyone out.").
- Say what empty / the default means.
- Env var names, file paths and unit names go in Nix comments, not descriptions.
- Every non-obvious key gets `ui.label`.
- Secrets stay visible; ports and plumbing go in `lib.neo.ui.groups.advanced`.

### Structural fixes

- **Schedule widget** (`ui.widget = "schedule"`): daily at / weekly on / every N
  hours, writes OnCalendar; raw expression stays available. Used by all four
  schedule options.
- **Choice where a free string is expected**: `vpn.vpnServiceProvider`,
  authentik `email.useTls`/`useSsl` → one "Encryption" choice,
  `core.timeZone` picker.

### Bug found during the audit

`nix/services/collabora/option.nix` declared `flake.modules.nixos.nextcloud-option`
(copied from nextcloud). Both definitions merge as deferred modules, so it
worked by accident; renamed to `collabora-option`.

---

## 3. Guides

### What a guide is

An outcome-focused page: *"Block ads for the whole home"*. It has a header
(services used, difficulty, time, which data it brings home), steps that link to
the exact settings, and checks ("open a news site on your phone — no ads").

### Declaration

Metadata in Nix, body in Markdown. Nix does not parse YAML front matter, and
keeping metadata in Nix lets eval validate it.

```nix
# nix/guides/adblock-at-home/default.nix   (auto-imported like services)
{...}: {
  flake.modules.nixos.guide-adblock-at-home = {lib, ...}: {
    neo.guides.adblock-at-home = lib.neo.mkGuide {
      title = "Block ads for the whole home";
      summary = "Every phone, TV and laptop at home without ads, without installing anything on them.";
      icon = ./icon.svg;                # optional, like mkServiceMeta.icon
      services = [ "pihole" ];          # required services (enable button in the header)
      optional = [ "tailscale" ];       # services the guide mentions
      privacy = [ "browsing" ];         # §4 categories
      difficulty = "easy";              # easy | medium | advanced
      minutes = 15;
      tags = [ "network" "privacy" "family" ];
      body = ./guide.md;
    };
  };
}
```

Guides live in `nix/guides/<id>/` (core) or anywhere in a plugin. `neo.guides`
is an `attrsOf` submodule, so plugins add guides the same way they add services.

### Markdown and links

CommonMark + tables, rendered by `pulldown-cmark` in the CLI (new dependency) and
sanitised. Images next to the `.md` are copied to the store and served like
service icons.

Cross-links use a small URI scheme, so the author never writes a UI route:

| Link | Renders as |
|---|---|
| `[Pi-hole](service:pihole)` | service chip with icon and status dot → service pane |
| `[local IP](option:pihole.localIP)` | opens the pane scrolled to that field, highlighted; shows the current value |
| `[Tailscale guide](guide:private-traffic-tailscale)` | guide card link |
| `[Calendar](privacy:calendar)` | privacy map tile |
| `[apply](action:apply)` | the existing Apply button (with its lock attr) |

Inline helpers:

- `{{domain}}`, `{{service.pihole.url}}`, `{{option.pihole.localIP}}`: current
  values, so steps say `https://pihole.example.com` instead of `<sub>.<domain>`.
- Callouts: `> [!tip]`, `> [!warning]`, `> [!check]`. GitHub syntax, so the
  `.md` also reads fine on GitHub.

**Validation** (flake check, like `service-targets-check.nix`): every `service:`,
`option:`, `guide:` and `privacy:` target must exist; `services` must be real
service names. Broken links fail `just check`, not the reader.

**Backlinks** are computed in extract, not written by hand:

- service pane → "Guides with Immich" (from `services` / `optional` and body links)
- guide → "Related guides" (shared services/tags + explicit `guide:` links both ways)
- privacy tile → guides tagged with that category

### Web UI

- **Guides** page in the navigator: card grid with search and filters (tag,
  difficulty, "uses only installed apps").
- **Guide page**: header (services with status, difficulty, time, data brought
  home, *Enable required apps* button, *Do it with AI* button), rendered body,
  related guides, "On this page" table of contents for long guides.
- **Service pane**: "Guides" strip under the description.
- Same Handlebars layout as the rest of the UI; progress/locks/htmx behave as
  everywhere else.

### Do it with AI

Each guide is also published as a Hermes skill `neo-guide-<id>` through the
existing collector (`nix/services/hermes/skills.nix`, `mkSkillMd`): the Markdown
body with links resolved to real URLs and option paths, plus a footer telling
Hermes how to run a guide:

1. Read current settings; skip steps that are already done.
2. Ask the user only what the guide cannot know (router model, phone OS).
3. Propose settings changes with `neo_settings_propose` (§3b); the user saves /
   applies from the card; wait for `neo_settings_status` = applied.
4. Run each `> [!check]` and report the result.
5. Never do destructive steps without explicit confirmation (as in the
   `neo-homeserver` skill).

Checks can carry a machine-readable command for Hermes:

````markdown
> [!check] Pi-hole answers DNS
> ```neo-check
> dig +short @{{option.pihole.localIP}} doubleclick.net   # expect 0.0.0.0
> ```
````

The UI shows the check as text; Hermes runs the command.

*Do it with AI* opens the agent chat (§3a) with the guide skill loaded.

---

## 3a. Agent chat in the neo web UI

A chat drawer inside the neo web UI, opened from:

- a guide: **Do it with AI** → session starts with `/neo-guide-<id>`
- a service pane: **Ask for help** → session starts with `/neo-<service>` plus
  context (service name, current settings with secrets redacted, unit health)
- the navigator: a general **Assistant** button

**Only shown when Hermes is enabled** (`neo.services.hermes.enabled`). With
Hermes off, these buttons, the per-guide AI hints and the drawer do not exist.
The guide page shows a single line "Enable the assistant to get step-by-step
help" linking to the Hermes pane.

### Transport: Hermes gateway API

Hermes' gateway (`api.enabled = true` on `gatewayPort`, already configured)
serves an OpenAI-compatible API (`gateway/platforms/api_server.py`):

| Endpoint | Use |
|---|---|
| `POST /v1/runs`, `GET /v1/runs/{id}/events` (SSE) | one agent turn with streamed text, tool calls and status |
| `POST /v1/runs/{id}/approval`, `/clarify`, `/stop` | answer an approval prompt / clarifying question, cancel |
| `/api/sessions/{id}` (+ `/messages`, `/chat/stream`) | persistent sessions: reopen a guide chat later |
| `POST /v1/chat/completions`, `/v1/responses` | fallback / simple clients |

The browser never talks to Hermes directly. neo web adds a proxy route
`/agent/*` → `http://127.0.0.1:<gatewayPort>/…`:

- **Auth**: neo web is already behind tinyauth; the proxy only exists in that
  process, so tinyauth applies automatically. The gateway key (`gatewayToken`, set by
  the operator) is added server-side as `Authorization:
  Bearer`; it never reaches the browser.
- **Allowlist**: the proxy forwards only the run, session and skill endpoints
  above, not the whole gateway.
- **Session**: `X-Hermes-Session-Id = neo-<context>-<id>` (e.g.
  `neo-guide-adblock-at-home`), so reopening the guide resumes the chat. A
  "New chat" button forks or starts a fresh session.
- **Streaming**: SSE is passed through unbuffered (Rocket stream response).

### Who applies changes

Hermes **proposes**, the user **applies**. Settings changes go through the
settings tool (§3b): Hermes never edits `settings.toml` or runs `neo activate`
for a settings change. The proposal shows up in the chat as a card, and the
user's click runs the exact code path of the settings form and the Apply button.

For other actions (restart a unit, run a command), Hermes' own approval flow
is used: the event stream emits `approval.request`, the drawer renders a card
(what Hermes wants to do in plain words, the command) with **Approve** /
**Deny**, and the answer goes to `/v1/runs/{id}/approval`. Clarifying questions
(`/clarify`) render as buttons when Hermes offers choices.

Approval policy: restarts and anything destructive always ask; read-only
commands (status, logs, `neo-check`) do not.

---

## 3b. Settings tool

A small tool set neo exposes to Hermes so the agent can change settings by
option path, without touching files. It works from the chat drawer and also
from Telegram (where the card becomes a link to the proposal page).

### Tools (MCP)

neo web serves an MCP endpoint; Hermes gets it in `config.yaml`
`mcp_servers.neo = { url; headers.Authorization }` (generated by the Hermes
module when both services are enabled).

| Tool | Effect |
|---|---|
| `neo_settings_get(paths)` | current values + defaults + descriptions for option paths (`services.pihole.localIP`); secrets come back as `"<set>"` / `"<unset>"`, never the value |
| `neo_settings_schema(service)` | the option tree of one service (types, choices, labels), from the same extract the form uses |
| `neo_settings_propose(summary, changes)` | create a proposal, **writes nothing**; returns `proposal_id` and a validation result |
| `neo_settings_status(proposal_id)` | `pending` / `saved` / `applied` / `rejected` / `stale`, plus activation result once applied |

`changes` is a list of operations on option paths:

```json
{
  "summary": "Turn on Pi-hole and use it for your home network",
  "changes": [
    { "op": "set",   "path": "services.pihole.enabled", "value": true },
    { "op": "set",   "path": "services.pihole.localIP", "value": "192.168.1.10" },
    { "op": "unset", "path": "services.pihole.upstream" },
    { "op": "generate", "path": "services.pihole.webPassword" }
  ]
}
```

- `set` / `unset` (back to the default) / `add` / `remove` (list items, attrsOf
  keys).
- `generate` fills a secret with the option's own `helper` (e.g.
  `randomToken`) at save time, so Hermes never sees or invents a secret value.
  A `set` on a secret path is refused; secrets the user must supply (an API
  key) are entered in the card.

### Proposal

Stored by neo web (in memory + `/run/neo/proposals/<id>.json`, expires after a
day). On `propose`, neo:

1. validates every path and value against the option schema (exists, type,
   enum/`ui.choices`, not `internal`) and rejects with a plain error the agent
   can fix;
2. records a fingerprint of the touched settings, so a later save detects that
   the user changed them meanwhile (`stale`, card offers "Recompute");
3. computes the semantic diff with the same code as the pending-changes view
   (`settings_semantic`), labels and descriptions resolved from the schema.

### Card in the chat

The chat drawer recognises a `neo_settings_propose` tool result in the run
event stream and swaps in `GET /proposals/<id>/card` (htmx, server-rendered):

- the summary, then one row per change: service icon, option **label**, old →
  new value, secrets as "will be generated" / "you enter it here" (input field);
- validation state, and any other unsaved changes already pending (the user
  sees that Apply activates those too, as today);
- buttons: **Save & apply**, **Save** (pending, apply later from the action
  bar), **Edit in form** (opens the service pane with the change prefilled, not
  saved), **Reject**;
- lock-aware like every other button: `data-neo-lock="system:sh"` for Save and
  `system` for Save & apply (`lock_attr`), disabled while an activation runs.

After the click, the card turns into a status line (saved / activating… /
applied ✓ / failed with the log link) and the result is posted back into the
run, so Hermes can continue with the guide's next step and its checks.

### Same route as the UI

Today `/save/<service>` and `/save-core/<section>` each load the document,
replace a table, call `route_secrets` and `finish_save`. Those steps move into
one function, used by the form, the proposal and later a CLI command:

```rust
// cli/src/commands/web/settings/edit.rs
pub enum Edit {
    ReplaceService { service, table },        // the settings form
    ReplaceCore { section, payload },         // core sections
    Patch { ops: Vec<PathOp> },               // proposals (and `neo settings set`)
}
pub async fn apply_edit(config: &AppConfig, edit: Edit) -> Result<(), EditError>;
```

`apply_edit` always:

- takes `LockSpec::system_shared()` (`settings_lock`),
- loads the document, applies the edit,
- runs the plugin table cleanup when `core.plugins` changes,
- calls `route_secrets` with the touched scope (secrets go to `secrets.yaml` /
  `secrets.sops.yaml` exactly like a form save),
- calls `finish_save` (sorted write, rekey, evaluator + schema-cache refresh,
  action bar broadcast).

**Save & apply** then calls `apply_or_activate` (the `/changes/apply` handler),
so activation, its lock, the progress monitor and the action bar behave exactly
as with the Apply button. Proposals add no second write path and no direct
file access.

### Security

- The MCP endpoint is under `/internal/mcp` on neo web's loopback listener and
  requires a bearer token (a `neo.core.secrets.templates` file readable by
  neo-web and hermes). SWAG denies `/internal/` so it is never reachable from
  outside, even behind tinyauth.
- The agent's token can only create proposals and read redacted settings; save
  and apply need the logged-in user's click in the web UI (tinyauth session).
- Proposals cannot touch `core.secrets.*`, `core.plugins`, `disko`, SSH
  `authorizedKeys` or the neo/tinyauth `enabled` switch (refused with a hint to
  use the UI), so the agent cannot lock the user out.

### UI

- Drawer on the right (full screen on mobile), Handlebars partial + one JS
  module (`static/agent_chat.js`), with `agent_chat.test.js` under
  `just test-widgets`.
- Messages render Markdown with the same renderer and link scheme as guides,
  so `option:pihole.localIP` in a Hermes answer becomes a clickable field link.
- Tool activity collapsed by default ("Checked Pi-hole status ✓").
- The guide page highlights the step Hermes is on, if the run reports it
  (skill emits `step: <n>` markers; optional).

### Open

- Check that the Hermes version Neo pins exposes the runs/approval endpoints
  with a bearer key (seen in the current `hermes-agent` input) and whether
  approval granularity can be configured per tool.
- Redaction of secrets in the context Neo sends: use the same secret paths as
  `neo.core.secrets` so nothing sensitive goes to the LLM provider.

### First guides

| id | Guide | Services / options |
|---|---|---|
| `private-traffic-tailscale` | Keep your app traffic private with Tailscale (devices talk to the server directly; certificates still need port 80 or Rathole) | tailscale `splitDns`, `ingress`, rathole `certificateOnly` |
| `adblock-at-home` | Block ads for the whole home | pihole; router DNS setting |
| `leave-google-photos` | Leave Google Photos | immich (+ phone app backup) |
| `party-photo-drop` | Collect photos from guests after a party | immich-drop |
| `family-calendar` | A shared family calendar on every phone | rustical, calino, ical-sync (school/holiday feeds), DAVx5 / iOS |
| `family-passwords` | A password manager for the whole family | vaultwarden |
| `go-paperless` | Scan letters and find them again | paperless |
| `location-timeline` | Your private location history | dawarich + OwnTracks |
| `offsite-backup` | Back up to a friend's server | backup |
| `reach-from-internet` | Make your apps reachable without opening the router | rathole / streamproxy, `ingress` |
| `sign-in-with-google` | Sign in with Google or Microsoft | tinyauth oauth, authentik |
| `private-search` | Private search as your browser default | searxng |
| `tv-without-sponsors` | YouTube on the TV without sponsor segments | isponsorblocktv |
| `office-together` | Edit office documents together | nextcloud + collabora |
| `private-vpn-browser` | Browse through a VPN without a VPN app | vpn + firefox/webtop `vpn.enabled` |

---

## 4. Privacy map

A page (and a dashboard teaser) showing the kinds of personal data people
usually hand to cloud companies, as a grid of tiles around a house.

Clicking a tile opens a panel:

- **What the cloud keeps**: "Google Calendar knows who you meet, when and where."
- **At home with**: the services tagged with that category (RustiCal, Calino,
  ical-sync), each with status (installed ✓ / Enable) and its tagline.
- **How to set it up**: the guides for that category, each with *Do it with AI*.

Tiles of categories already covered by enabled services are highlighted. That
gives a "how much of my data is at home" overview without any scoring.

### Categories

Declared centrally (`nix/lib/privacy.nix`, extendable by plugins via
`neo.privacy.categories.<id>`), each with `label`, `icon`, `cloud` (what the big
providers keep), `examples` (Google Calendar, iCloud…), `body` (optional `.md`).

| id | Label | Services today |
|---|---|---|
| `calendar` | Calendar | rustical, calino, ical-sync, nextcloud |
| `contacts` | Contacts | rustical, nextcloud |
| `photos` | Photos & videos | immich, immich-drop |
| `passwords` | Passwords | vaultwarden |
| `location` | Location history | dawarich |
| `documents` | Letters & documents | paperless, stirling-pdf, bentopdf |
| `files` | Files | nextcloud, syncthing, filebrowser, dufs |
| `notes` | Notes & wiki | docmost |
| `tasks` | To-dos | vikunja |
| `bookmarks` | Bookmarks & read-later | karakeep |
| `search` | Web search | searxng |
| `browsing` | Browsing & ads | pihole, vpn |
| `recipes` | Recipes & meal plans | mealie |
| `code` | Code & config | gitea |
| `automation` | Automations | activepieces |
| `assistant` | AI assistant | hermes |

Services opt in with `mkServiceMeta { privacy = [ "calendar" "contacts" ]; }`;
eval checks that the ids exist.

---

## Rollout

1. **Meta split**: add `tagline`, `notes`, `replaces`, `privacy` to
   `mkServiceMeta`, extract and templates; rewrite all 44 service texts. Fix the
   collabora module name.
2. **Option texts**: rewrite per the rules, add labels/groups/`visibleWhen`;
   style rule in AGENTS.md. Secret generation and the schedule widget as
   separate changes.
3. **Guides**: `mkGuide`, `neo.guides`, extract, link validation check,
   pulldown-cmark renderer, Guides page, service-pane strip; first 3 guides
   (`adblock-at-home`, `private-traffic-tailscale`, `family-calendar`).
4. **Settings tool**: `settings/edit.rs` (`apply_edit`, form routes moved onto
   it), proposals + card route, MCP endpoint, Hermes `mcp_servers` wiring.
5. **Agent chat**: `/agent/*` proxy, chat drawer with proposal and approval
   cards, *Ask for help* on service panes; guide → Hermes skill and *Do it with
   AI*, `neo-check` blocks.
6. **Privacy map**: categories, tagging, page and panel.
7. Remaining guides.

## Decisions

- **Applying changes**: Hermes proposes settings changes through the settings
  tool; the user saves / applies from a card in the chat, through the same
  `apply_edit` + `apply_or_activate` path as the form and the Apply button
  (§3b). AI features only exist when Hermes is enabled.
- **Translations**: English only for now. Keep the door open: `body` accepts a
  path today and may later accept `{ en = ./guide.md; de = ./guide.de.md; }`;
  `title`/`summary` likewise. Link targets and `{{…}}` placeholders are
  language-neutral, so translated files reuse them.
- **Public site**: not now. Keep the renderer free of web-UI state so a static
  export (`neo guides export <dir>`, live values replaced by examples) can be
  added later.
