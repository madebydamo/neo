# Plugin instances: stable ids, editing, and prefixed services

Status: accepted, in progress

## Problems today

1. **One copy per repo.** A plugin's services have fixed names (`neo.services.example`, container `example`, unit `docker-example`, appdata `…/example`, subdomain `example`). Importing the same repo twice, for example `portrait` on `main` and on `dev`, declares every option twice and evaluation fails. Even if it evaluated, both copies would fight over the same containers, units and directories.
2. **No editing.** `core.plugins` is a list of URLs. Switching a branch means Remove + Add. Inputs are `plugin0`, `plugin1`, … by list position, so removing one entry renumbers every later input. That rewrites `flake.nix`, renames nodes in `flake.lock`, and the commit is much bigger than the change.
3. **Attribution is by URL and store path.** `extract_plugin_inventory.nix` matches option declarations against each input's `outPath`. Two instances of the same revision have the same store path, so they cannot be told apart.

## Goals

- Every plugin entry has a **stable id**. Its flake input is named after the id, not after its position.
- A plugin's **URL can be edited in place**. Changing the branch touches one line in `settings.toml`, one input in `flake.nix` and that input's lock node.
- A plugin can be imported **again with a prefix**. Its services become `dev-example`, with their own options, containers, units, appdata and subdomain.
- Existing installs and plugins keep working without changes. A plugin that has not opted into prefixing can still be imported once.

## Non-goals

- Running unmodified plugins twice. The NixOS module system has no namespaces, and plugins write `systemd.services.docker-example` and `http://example:80` as literals. Neo cannot rename those from the outside. Prefixing needs the plugin to name its services through a helper, so it is opt-in per plugin.
- Moving appdata when a prefix changes (see "Changing a prefix").

---

## 1. Settings model

`core.plugins` goes from a list to a table keyed by id:

```toml
[core.plugins.portrait]
url = "github:madebydamo/portrait"

[core.plugins.portrait-dev]
url = "github:madebydamo/portrait/dev"
prefix = "dev"
```

Option (`nix/modules/core/options.nix`):

```nix
options.neo.core.plugins = mkOption {
  type = types.attrsOf (types.submodule {
    options = {
      url = mkOption { type = types.str; … };
      prefix = mkOption { type = types.str; default = ""; … };   # [a-z0-9]+ or ""
      # later: enabled = true; to park a plugin without deleting it
    };
  });
  default = {};
  ui = lib.neo.ui.mkUi { widget = "pluginList"; … };
};
```

- **id**: `[a-z][a-z0-9-]*`, unique. The web UI suggests the repository name (`plugin_label`) and appends `-2`, `-3` on a clash, or uses `<repo>-<prefix>` when a prefix is set. The id is editable only while adding.
- **prefix**: empty means "service names as the plugin wrote them". `dev` turns `example` into `dev-example`. The `-` separator is added by Neo, so operators never type it. Two instances of the same plugin may not have the same prefix, and only one instance per plugin may have an empty prefix. Both rules are checked in the form and again at eval (assertion).
- Order no longer matters, and nothing depends on it. `toml_edit` keeps the tables in the order they were added. The UI shows them in that order.

### Migration

`neo migrate` (`cli/src/commands/migrate.rs`) and the settings loader turn the old list form into tables:

```toml
plugins = ["github:madebydamo/highsea.neo", "git+file:/home/me/portrait"]
# →
[core.plugins.highsea-neo]
url = "github:madebydamo/highsea.neo"
[core.plugins.portrait]
url = "git+file:/home/me/portrait"
```

The input names change once (`plugin0` → `plugin-highsea-neo`), so the first commit after the upgrade rewrites the plugin nodes in `flake.lock`. That is a one-time cost.

Until the migration has run, `inputs.nix` and `nixos.nix` in the homeserver template accept both forms. A list becomes ids `p0`, `p1`, … so an un-migrated config still evaluates.

## 2. Flake wiring (homeserver template)

`templates/homeserver/modules/inputs.nix`:

```nix
plugins = core.plugins or {};   # after list→attrs normalisation
flake-file.inputs = … // lib.mapAttrs' (id: p: {
  name = "plugin-${id}";
  value = { url = p.url; inputs.neo.follows = "neo"; inputs.nixpkgs.follows = "nixpkgs"; inputs.flake-file.follows = "flake-file"; };
}) plugins;
```

`flake.nix` is generated with sorted attribute names, so adding or editing one plugin changes one block.

`templates/homeserver/modules/nixos.nix` stops scanning `attrNames inputs` and walks `core.plugins` from `settings.toml` (the same file `inputs.nix` reads):

```nix
pluginModule = id: p: let
  flake = inputs."plugin-${id}";
  instance = { inherit id; inherit (p) url; prefix = p.prefix or ""; };
in
  if flake ? neoPlugin
  then flake.neoPlugin.module instance       # instance-aware plugin
  else if instance.prefix == ""
  then flake.nixosModules.default            # legacy plugin, imported once
  else throw "Plugin '${id}' (${p.url}) does not support a prefix. Remove the prefix or update the plugin.";
```

Edge case: the same legacy plugin imported twice without a prefix fails with the usual "option declared more than once". The form catches this first: it allows only one prefix-less instance per URL (ignoring the ref).

## 3. Plugin side: the `plugin` argument

### What authors write

Neo ships a flake-parts module `inputs.neo.flakeModules.plugin` that the plugin template imports. It adds an option `neo.plugin.modules.<name>`: NixOS modules that receive one more argument, `plugin`:

```nix
# modules/services/example/default.nix
{...}: {
  neo.plugin.modules.example = {config, lib, plugin, ...}: let
    name = plugin.name "example";            # "example" or "dev-example"
    cfg = config.neo.services.${name};
    appdata = "${config.neo.core.volumes.appdata}/${name}";
  in {
    config = lib.mkIf cfg.enabled {
      virtualisation.oci-containers.containers.${name} = { … };
      systemd.services."docker-${name}".preStart = …;
    };
  };
}
```

`plugin` holds:

| Attribute | Value |
|-----------|-------|
| `plugin.name "x"` | `x`, or `<prefix>-x` |
| `plugin.prefix` | `""` or `"dev"` |
| `plugin.id` | the settings id (`portrait-dev`) |
| `plugin.url` | the flake URL |
| `plugin.label` | human suffix for titles: `""` or `" (dev)"` |

Rule for authors: **every** name that ends up in the shared system goes through `plugin.name`. That covers the `neo.services.*` key, containers, units, appdata, the default subdomain, hostnames used between the plugin's own containers (`http://${plugin.name "prowlarr"}:9696`), users and groups, secret template names, guide ids and `service:` links. References to Neo core services (`swag`, `tinyauth`) stay as they are.

### How Neo provides it

`lib.neo.plugin.instantiate instance modules` (new, `nix/lib/plugin.nix`) turns the raw modules into one NixOS module:

```nix
{ imports = lib.mapAttrsToList (n: m: {
    _file = "neo-plugin:${instance.id}#${n}";
    key = "neo-plugin:${instance.id}:${n}";
    imports = [ (withPluginArg pluginArgs m) ];
  }) modules; }
```

`withPluginArg` wraps a function module so that the module system still sees its other arguments (`lib.setFunctionArgs` with `plugin` removed) and passes `plugin` itself. `_module.args.plugin` cannot be used, because module args are global and two instances would clash.

The per-instance `key` keeps two instances from being deduplicated. The `_file` puts the instance id into every option declaration, which the inventory uses (section 5). One consequence for authors: a file pulled in with `imports = [ ./x.nix ]` by path is deduplicated by its path, so the second instance would silently drop it. Plugins already must not import siblings, and the docs will say so explicitly.

The flake module also exports:

```nix
flake.neoPlugin = { module = instance: lib.neo.plugin.instantiate instance config.neo.plugin.modules; };
flake.nixosModules.default = flake.neoPlugin.module { id = "default"; prefix = ""; url = ""; };  # older Neo hosts
```

Plugins that keep using `flake.modules.nixos.*` stay legacy: one instance, no prefix. Both kinds can be mixed in one plugin while porting, but only `neo.plugin.modules` get the `plugin` argument, so a plugin is prefix-safe only when everything has moved.

### lib.neo helpers

Most helpers take names from their arguments, so they work once the author passes `name`. Only defaults change:

- `mkReverseProxyOptions { subdomain = name; }`: the template passes `name`, not `"example"`.
- `mkServiceMeta`: gets an optional `instance` (or `title`) so the grid card reads "Portrait (dev)" and the plugin badge shows the instance.
- `mkServiceSkill { service = name; }`: the skill becomes `neo-dev-example`.
- `mkContainerDefinitions { ${name} = "image"; }`: the container key is the name, unchanged otherwise.

### Catching missed literals

A plugin that forgets `plugin.name` in one place still evaluates in some cases. For example, two identical `volumes` lists merge silently. Neo adds `lib.neo.plugin.mkInstanceCheck` for the plugin template's `checks`:

1. It evaluates a stub host with the plugin once (prefix `""`) and once twice (prefixes `a` and `b`), with every plugin service enabled.
2. It fails if the second evaluation errors, or if any of these sets did not exactly double: `virtualisation.oci-containers.containers`, `systemd.services`, `systemd.timers`, `users.users`, `neo.services`, `neo.guides`.
3. On failure it lists the names that were not prefixed.

`nix flake check` in the plugin template runs it, so authors find leaks without a VM.

## 4. Web UI: plugin cards

The `pluginList` widget (`cli/templates/options/widgets/plugin_list.*`) changes from a list of strings to an id-keyed editor:

```
┌ portrait ──────────────────────────────── [Edit] [Remove] ┐
│ github:madebydamo/portrait                                │
│ Services: portrait                                        │
└───────────────────────────────────────────────────────────┘
┌ portrait-dev   prefix: dev ────────────── [Edit] [Remove] ┐
│ github:madebydamo/portrait/dev                            │
│ Services: dev-portrait                                    │
└───────────────────────────────────────────────────────────┘
[ github:user/repo … ] [+ Add plugin]
```

- **Add**: URL field, then a small form with id (pre-filled) and prefix (empty). If the URL's repository is already installed, the form says so, pre-fills a prefix suggestion (the branch name if the URL has one) and requires a non-empty prefix when the existing instance has none.
- **Edit**: the card becomes a URL field plus a **Branch** field. The branch field reads the ref out of the URL and writes it back in the right form for the scheme: `github:o/r/<ref>`, `gitlab:o/r/<ref>`, `?ref=<ref>` for `git+…`. Not shown for `path:`. A single `with_ref`/`ref_of` pair in Rust (`plugins.rs`) and in the widget JS keeps both sides in sync, with tests on every scheme. Saving an edit keeps the id, so only the `url` line changes.
- **Prefix** is shown, not editable, after the plugin has been saved (see below).
- **Update this plugin** (optional extra): runs `nix flake update plugin-<id>` instead of updating every input. This is possible now that input names are stable.
- **Remove** stays as it is, but the "tables to delete" list now comes from instance ownership (section 5). That also removes the prefixed tables of exactly that instance.
- Services grid source filter and badges: one entry per instance (`portrait`, `portrait-dev`), labelled by id, so there is no more URL disambiguation.

### Editing a URL that drops a service

When a different branch no longer declares a service that has a `[services.x]` table, the next evaluation fails on the unknown key. The edit save cannot know this before the new revision is fetched. Plan:

- The save goes through as it does today.
- When the activation build fails with an undeclared `services.<name>` option that belongs to a plugin instance, the error toast offers **Remove leftover settings for `<name>`** (the same `strip_service_tables` path).

### Changing a prefix

A prefix change renames every service, so its settings tables, appdata, subdomains and secrets all move. Version 1 does not allow it: the prefix is read-only once saved, and the card says "Remove and add again to change the prefix". A later version can rename `[services.<old>]` → `[services.<new>]` in the same save and leave appdata under the old name with a warning.

## 5. Inventory and ownership

`extract_plugin_inventory.nix` becomes simpler and exact:

- Instances come from the inputs named `plugin-<id>`. The URL still comes from the lock / `flake.nix` of the *applied* config, never from live settings (unchanged rule).
- Instance-aware plugins: a service belongs to the instance whose `neo-plugin:<id>#…` appears in the option's `declarations`.
- Legacy plugins: the store-path match as today, keyed by id instead of URL.
- Output: `{ plugins = [ { id; url; prefix; services; } ]; owners.<service> = [ id … ]; }`.

Rust (`plugins.rs`, `settings/edit.rs`): `plugins_from_doc` returns `Vec<PluginEntry { id, url, prefix }>`. `services_to_remove` compares **ids**, not URLs. A URL edit therefore never strips tables. A removal strips only that instance's tables. `normalize_plugin_url` stays for the duplicate-URL check.

## 6. Other places that read `core.plugins`

| File | Change |
|------|--------|
| `cli/src/commands/web/diff/flake_lock.rs` | Input `plugin-<id>` → label `<id>`. Drop the index parsing. |
| `cli/src/commands/restore/local.rs` (`missing_local_plugins`) | Read the url of each table. |
| `cli/src/commands/web/proposals.rs` | `core.plugins` stays refused for the assistant. Check the new path shape (`core.plugins.<id>.url`). |
| `cli/src/commands/web/util/validate.rs` | Validate id, prefix and the uniqueness rules. |
| `nix/output/templates-check.nix` | Cover the attrs form, the list fallback and an instance-aware plugin with two prefixes. |
| `docs/PLUGINS.md` | Install/edit/branch/prefix for users. `neo.plugin.modules`, `plugin.name`, the check and the "no path imports" rule for authors. "Make your plugin prefix-safe" checklist. |
| `AGENTS.md` | One paragraph on plugin instances. Update the `pluginList` widget row. |

## 7. Order of work

1. **Stable ids + edit** (no plugin changes needed): settings model with list fallback, migration, `inputs.nix`/`nixos.nix`, inventory by id, Rust ownership by id, widget Edit + Branch field, flake-lock diff labels. This alone fixes the branch-switch commit size.
2. **Instance API in Neo**: `nix/lib/plugin.nix` (`instantiate`, `withPluginArg`, `mkInstanceCheck`), `flakeModules.plugin`, `nixos.nix` choosing `neoPlugin` vs legacy, prefix field and rules in the form, assertions.
3. **Template + docs**: port `templates/plugin` to `neo.plugin.modules` + `plugin.name`, add the instance check to its `checks`, rewrite the author half of `docs/PLUGINS.md`.
4. **Port real plugins**: `portrait` first (small, the branch use case), then `highsea.neo` (many cross-service hostnames, a good test for the check).
5. **VM acceptance**: `portrait` (no prefix) + `portrait` on `dev` with prefix `dev`. Expect `docker-portrait` and `docker-dev-portrait` active, `https://portrait.<domain>` and `https://dev-portrait.<domain>` serving different revisions, an edit to the dev branch changing only its input, and Remove of the dev instance stripping only `[services.dev-portrait]`.

## Decisions

- **Prefix, not suffix**: `dev-portrait`. It reads well next to the URL field.
- **Default prefix for a second instance**: the branch name from the URL (sanitised to `[a-z0-9]`), editable while adding.
- **Old plugins must keep working**: plugins without `neoPlugin` are imported as before (once, no prefix). Prefix support is opt-in for plugin authors.
- Later, not now: `enabled = false` per plugin.
