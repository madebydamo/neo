# Web UI option metadata and shared front-end pieces

Status: implemented. How the settings form renders options from metadata
declared in Nix, how to add a special UI, and the shared templates and scripts
every page uses. The `lib.neo.ui` function reference is in
[LIB.md](../LIB.md#web-ui-metadata-libneoui).

## Rule

Declare presentation next to the option in Nix. Extract serializes it; the form
interprets it generically. **Do not** hardcode service or option names in
`option_form.js` or `extract_service_options.nix`.

| Field | Role |
|-------|------|
| `rank` | Sibling sort order (see `nix/lib/option.nix`) |
| `helper` | Fill-assist (`lib.neo.helpers.*`, scripts under `nix/lib/helpers/`) |
| `ui` | Widgets, dynamic multi-select choices, key linkage, save prune (`nix/lib/ui.nix`) |

Rank bands for the top level of `neo.services.<name>`: in
[AGENTS.md](../../AGENTS.md#web-ui-option-metadata) and `nix/lib/option.nix`.

## Sections and labels (`ui.group` / `label` / `summary` / `choiceLabels` / `visibleWhen`)

- The service pane shows **General** (ungrouped options, always open), then
  collapsible **groups**.
- `ui.group = lib.neo.ui.groups.access` (or `mkGroup { id; label; description;
  icon; rank; }`) on an option puts it and all its descendants in that section.
- Generated option sets already do this: `mkReverseProxyOptions` /
  `mkVpnOptions` → access, `mkSkillOptions` → assistant,
  `mkContainerDefinitions` → advanced.
- The section split and humanized labels happen in
  `cli/src/commands/web/nix/sections.rs`.
- Group `icon` takes a path or URL like `mkServiceMeta.icon`. Built-ins are
  `lib.neo.ui.icons.<name>` (every `nix/lib/icons/<name>.svg`); a plugin can
  pass its own `./icon.svg`. SVGs are drawn as a `currentColor` mask so they
  follow the theme; other images render as they are.

Fields:

- `label`: human name (default: humanized last path segment).
- `summary = true`: value chip on the collapsed group header.
- `choiceLabels = { web = "Internet"; }`: display names for enum / choice values.
- `visibleWhen = "enabled"`: sibling bool; the field hides while it is false.
  `visibleWhen = "type=entra"` hides it unless the sibling equals that value,
  `"format=files,encrypted"` unless it equals one of them. Works inside attrsOf
  entries too.

## `ui.choices` (multi-select)

- On a `listOf str` (or a nested field), set `ui.choices = "authApps"` (named
  provider) or `ui.choices = [ "a" "b" ]`.
- Extract attaches `type.values`; templates render a checkbox grid when
  `type.values` is set.
- Named providers live in `extract_service_options.nix` → `choiceProviders`.
  Today: **`authApps`** = enabled services with reverse-proxy auth, excluding
  tinyauth. Add new providers there; never `if service == "…"`.

## `ui.keysFrom`

AttrsOf keys follow another option:

```nix
ui.keysFrom = lib.neo.ui.mkKeysFrom {
  option = "users";
  extract = "beforeColon";  # or "identity"
};
```

Extract prunes orphan keys from `current`; the form re-syncs when the source
list/attrs change.

## `ui.widget` (composite editors)

| Widget | Use |
|--------|-----|
| `exclusiveListPair` | attrsOf submodule with exclusive list fields + open mode (e.g. tinyauth `access` allow/block) |
| `pluginList` | `core.plugins` instances `{ <id> = { url; prefix; }; }`: cards with add (optional prefix, suggested from the branch when the plugin is already there), in-place edit of address / branch, and remove with uninstall confirm |
| `primaryItemList` | listOf scalars; the first entry is the primary (badge from `entryLabel`, e.g. Hermes `telegramAllowedUserId` home channel) |
| `providerAuth` | submodule of provider + API key and/or OAuth login + model (Hermes `llm`). Catalog rows declare `hasApiKey` / `hasOauth` / `oauthFlow` / `needsBaseUrl`; child option descriptions render as ⓘ on each input; `ui.oauth.script` runs status/login/refresh |
| `proxyRouteList` | attrsOf str of public domain → plain `http://` upstream (SWAG `proxyPass`). Route cards `https://domain → TLS at SWAG → upstream` with add/remove, inline checks (hostname only, `http://host[:port]`, duplicates = error; `https://`, path, loopback = warning; no port = hint) and one-click fixes. Errors block save through the optional widget `validate(name) → string[]` hook |

Implementation:

- Files: `cli/templates/options/widgets/<name>.html.hbs` + `<name>.js` (file
  names in snake_case, e.g. `proxy_route_list.js`; registers on `NeoWidgets`) +
  `<name>.test.js`.
- `option_form.js` mixes in registered widgets and dispatches init/save/reset on
  `ui.widget` only.
- A widget may define `validate(name)` returning error strings; `save()` refuses
  (toast + error flash) while any remain.
- Dispatch in `attrs_of.html.hbs` / field templates on `ui.widget`, not on
  option names.
- Widget tests run in the real `option_form.js` through
  `widgets/test/harness.js`. Run `just test-widgets`.

## Adding a new special UI (checklist)

1. Prefer composing **choices** + **keysFrom** + **save** only; add a **widget**
   only if the generic type editor is not enough.
2. Declare `ui` on the option in `option.nix` via `lib.neo.ui.mkUi { ... }`.
3. If you need a new dynamic list, add a **choice provider** in extract (one
   attr, reused by any service).
4. If you need a new composite editor: one Handlebars partial + one JS module
   under `options/widgets/` that `NeoWidgets.register`s by `ui.widget` name, a
   colocated `*.test.js`, and a `<script>` in `configuration.html.hbs` after
   `registry.js`. No `if (name === "access")`.
5. Document the widget/provider in this doc (and the widget list in
   [LIB.md](../LIB.md)).

Reference consumers:

- `nix/services/tinyauth/option.nix`: `access` + `ui.choices = "authApps"`.
- `nix/services/hermes/option.nix`: `telegramAllowedUserId` + `primaryItemList`.
- `nix/services/swag/option.nix`: `proxyPass` + `proxyRouteList`.

## Shared templates

- Fields of a submodule entry render once: `options/entry_field` (`parent`,
  `accessor`, `target_key`), used by `attrs_of` and `list_of`, so secrets,
  `visibleWhen` and helpers work in both.
- Inputs use `options/secret_toggle` and `options/helper_button`.
- Page chrome: `partials/head_meta`, `partials/vendor_scripts` (the CDN
  htmx/Alpine tags and their versions), `partials/changes_modal`,
  `partials/eval_error_actions`, `partials/core_card`,
  `partials/section_heading`.
- Handlebars partials take hash params; a class passed in one must be written
  out in full so Tailwind sees it.

## Shared scripts (`static/neo_util.js`)

`NeoUtil` is loaded first on both pages:

- `esc`, `store('local'|'session')` (storage that never throws), `onReady`,
  `markStandalone`, `readJson(res, fallback)`, `h(tag, props, ...children)`
  (element builder), `isContentSwap(target)`, and the global `neoToast`.
- `reloadPane`: the leave guard asks, so callers do not confirm.
- `confirm(message, opts)`, `alert`, `prompt(message, value)` (Promises) and
  `dialog(opts)`: an in-page modal instead of the browser's own dialogs, which
  agents driving the browser cannot click. A blank line after the first
  paragraph makes it the title; `okLabel`, `danger` (red OK).
- `hx-confirm` goes through the same dialog (the `htmx:confirm` hook in
  `wireHooks`): its OK button reads `data-neo-confirm-ok`, else the button's
  short text; it is red for error buttons or delete/discard questions.
  `guardRequests(fn)` adds a check every htmx request passes first (the
  unsaved-changes guard in `configuration.js`). Never call `window.confirm`,
  `alert` or `prompt`. Hard reloads with unsaved edits still get the browser's
  own leave warning (`beforeunload` cannot be replaced).
- `wireHooks` handles the markup hooks `data-neo-reload`, `-scroll-to`, `-copy`
  and `-remove-closest`.

Markup rendered from Rust carries `data-neo-*` attributes for delegated
handlers, never inline `onclick` (`hx-on::` stays). Buttons whose output lands
in `#changes-body` use
`hx-on::after-request="NeoOpMonitor.changesAfterRequest(event, 'Title')"`.

## Loading feedback (`cli/static/nav_progress.js`)

It hooks htmx events generically. Any request swapping into `#config-content`:

- drives the navbar spinner (`onBusyChange`);
- gets a delayed top progress bar (`#neo-progress`);
- gives the clicked element a pressed/spinner state (`data-neo-loading`);
- veils the target (`aria-busy` + `.neo-swap-busy`);
- shows a "Loading…" label after ~1.5 s.

Opt other requests in with `data-neo-progress` (on the element or an ancestor),
out with `data-neo-progress="false"`. Do not add per-page loading hacks.
