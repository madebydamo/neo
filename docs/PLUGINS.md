# Plugins

Plugins add things to Neo without forking it: more apps, your own projects, extra system configuration. A plugin is a Nix flake. Neo imports it into your homeserver, and its services get the same HTTPS, login, updates, status page and backups as the built-in ones.

The first half of this page is for people who **use** plugins. The second half is for people who **write** them, with a full `lib.neo` reference.

## Contents

**Using plugins**

- [Awesome plugins](#awesome-plugins)
- [Install a plugin](#install-a-plugin)
- [Private plugins](#private-plugins)
- [Local plugins](#local-plugins)
- [After installing](#after-installing)
- [Change the address or branch](#change-the-address-or-branch)
- [Add the same plugin twice](#add-the-same-plugin-twice)
- [Updating plugins](#updating-plugins)
- [Removing a plugin](#removing-a-plugin)

**For plugin authors**

- [Create a plugin](#create-a-plugin)
- [How Neo loads a plugin](#how-neo-loads-a-plugin)
- [Layout rules](#layout-rules)
- [Prefixes: add a plugin twice](#prefixes-add-a-plugin-twice)
- [Development loop](#development-loop)
- [Turn your project into a Neo plugin (checklist)](#turn-your-project-into-a-neo-plugin-checklist)
- [Service files in detail](#service-files-in-detail)
- [Other service shapes](#other-service-shapes)
- [Verify on the host](#verify-on-the-host)
- [`config.neo` values a plugin reads](#configneo-values-a-plugin-reads)
- [`lib.neo` reference](#libneo-reference)
- [Real-world plugins](#real-world-plugins)

---

## Awesome plugins

| Plugin | What it adds |
|--------|--------------|
| **[highsea.neo](https://github.com/madebydamo/highsea.neo)** | Media stack: Jellyfin, Sonarr, Radarr, Lidarr, Prowlarr, qBittorrent, Bazarr, Tdarr, Seerr, FlareSolverr, Audiobookshelf and more, auto-wired with declarr |
| **[portrait](https://github.com/madebydamo/portrait)** | A personal site built and packaged by its own flake, served behind Neo's HTTPS |

Know a good plugin? Open an issue or PR on Neo to get it listed.

## Install a plugin

1. Open the Neo web UI.
2. Go to **Settings → System**. The **Plugins** list is in the General section.
3. Type the flake URL into the field and click **+ Add plugin**. Leave the prefix field empty:

   ```text
   github:madebydamo/highsea.neo
   ```

   A branch goes at the end for `github:` / `gitlab:` URLs (`github:madebydamo/portrait/dev`) and as `?ref=dev` for `git+…` URLs.

4. Click **Save**, then **Activate** in the top bar. Neo fetches the plugin and rebuilds the system.
5. Open the **Services** tab. The plugin's services are listed next to the built-in ones. Enable the ones you want, Save, Activate.

After the first activation, the plugin card lists the services it adds. Before that it says "No services attributed to this plugin in the current eval".

Plugins are stored in `settings.toml`, one table per plugin, so you can also edit them there. The name after `core.plugins.` is the plugin's id; Neo picks it from the repository name when you add it in the web UI.

```toml
[core.plugins.highsea-neo]
url = "github:madebydamo/highsea.neo"
```

Configs from before Neo had ids (`plugins = [ "github:…", … ]`) still work. `neo update` turns the list into tables once.

## Private plugins

The homeserver fetches plugins with the machine git identity (**Settings → System → Git**, `[core.git]` in `settings.toml`). Use one of these:

- **SSH key.** Copy the **Git SSH public key** from the Git section into your forge, as an account SSH key or a read-only deploy key on the plugin repo. Then use an SSH URL:

  ```text
  git+ssh://git@github.com/you/my-plugin
  ```

  github.com and gitlab.com are trusted out of the box. For any other forge, add its host key to `core.git.knownHosts` first: the value is the output of `ssh-keyscan -t ed25519 git.example.com` without the leading host name. Use `git.example.com:2222` as the key for a non-standard port. **Activate the new host key before adding the plugin**: the build fetches plugins before the new key is installed.

- **Access token.** Add a token for the host and keep the short URL:

  ```toml
  [core.git.tokens]
  "github.com" = "github_pat_…"   # fine-grained, read-only contents on the plugin repos
  ```

  ```text
  github:you/my-plugin
  ```

  The same token works for `git+https://` URLs. GitLab tokens need the `PAT:` prefix for `gitlab:` URLs.

Only the build user (`homeserver`) and Hermes (when enabled) can read these credentials. Root does not need them: the switch step reuses inputs the build already fetched.

## Local plugins

A plugin on the homeserver's disk (for development, or code you do not want to publish):

```text
git+file:/home/you/projects/my-plugin
```

or

```text
path:/home/you/projects/my-plugin
```

With `git+file:` Nix only sees files tracked by git, so `git add` new files. `path:` copies the whole directory. The path has to exist on the machine that builds the config.

You can install several plugins. Their order does not matter.

## After installing

- Plugin services show a badge with the plugin's id on the **Services** grid. The source filter offers **All sources**, **Core**, and one entry per plugin.
- Their options open in the same form as built-in services, including HTTPS, login, "Reachable from", the AI assistant and container images.
- HTTPS and login behave like the rest of Neo when the plugin uses Neo's reverse-proxy helpers (all listed plugins do).

## Change the address or branch

Click **Edit** on the plugin card. Change the **Address**, or type another **Branch** (empty means the repository's default branch), click **Done**, then **Save** and **Activate**.

The plugin keeps its id, so only its `url` line in `settings.toml` and its own entry in `flake.lock` change, and its settings stay. If the new branch no longer has a service you had set up, the build fails on the leftover `[services.<name>]` table: delete that table, or switch back.

## Add the same plugin twice

To run, for example, a `dev` branch next to `main`, add the plugin again with a **prefix**:

1. Type the second URL (`github:madebydamo/portrait/dev`). Neo notices the plugin is already there and fills the prefix field with the branch name.
2. Click **+ Add plugin**, **Save**, **Activate**.

```toml
[core.plugins.portrait]
url = "github:madebydamo/portrait"

[core.plugins.portrait-dev]
url = "github:madebydamo/portrait/dev"
prefix = "dev"
```

Every app of the second copy gets the prefix in front of its name: `dev-portrait-server` in the Services grid and `settings.toml`, its own containers, data folder and the subdomain `dev-portrait`. The prefix uses lowercase letters and digits and starts with a letter. It cannot be changed later, because it is part of every name: remove the copy and add it again instead.

Only plugins written for this can be added twice (see [Prefixes](#prefixes-add-a-plugin-twice)). Others fail to build with "does not support a prefix"; they still work once without a prefix.

## Updating plugins

Plugins are flake inputs of your config (`plugin-<id>`), pinned in its `flake.lock`. **Update** in the top bar (or `neo update` on the command line) refreshes all inputs, Neo and plugins alike. Then **Activate**. The scheduled system updater does the same when it is enabled.

## Removing a plugin

1. Go to **Settings → System → Plugins**.
2. Click **Remove** on the plugin card.
3. Neo shows the `[services.<name>]` tables it will delete from `settings.toml` because only this plugin declared them. For a prefixed copy that is only its own `dev-…` tables. Confirm with **Remove plugin**. Appdata directories are kept.
4. **Save**, then **Activate**.

Do not delete a plugin from `settings.toml` by hand without also removing its service tables. Leftover keys for options that no longer exist make Nix evaluation fail. The web UI does both in one save.

---

# For plugin authors

Everything below is for people writing a plugin, including coding agents asked to "turn this project into a Neo plugin". The checklist is the part to follow step by step; the reference after it lists every helper Neo exports.

Also read [AGENTS.md](../AGENTS.md): plugins follow the same rules as core services in `nix/services/`.

## Create a plugin

```bash
mkdir my-plugin && cd my-plugin
nix flake init -t github:madebydamo/neo#plugin
git init && git add -A
```

The template contains:

| File | Role |
|------|------|
| `flake.nix` | Generated by flake-file. **Do not edit**; run `nix run .#write-flake` after changing `modules/inputs.nix` |
| `modules/imports.nix` | Imports `flake-file.flakeModules.dendritic` and `neo.flakeModules.plugin` |
| `modules/inputs.nix` | Flake inputs (`nixpkgs`, `neo`, plus yours) as `flake-file.inputs` |
| `modules/nixos.nix` | Sets `systems` and the formatter. `neo.flakeModules.plugin` exports every `neoPlugin.modules.*` |
| `modules/services/example/option.nix` | Options: `enabled`, reverse proxy, containers, appdata, metadata, Hermes skill |
| `modules/services/example/default.nix` | Implementation: `mkEnsureDirs` for appdata, an OCI container on `internal`, `PUID`/`PGID` from `neo.core.uid`/`gid`, `TZ` from `neo.core.timeZone` |
| `modules/services/example/swag.nix` | Reverse proxy vhost via `lib.neo.mkSubdomainProxyConf` |
| `modules/services/example/skills.nix` | Hermes skill via `lib.neo.mkServiceSkill` |
| `modules/services/example/icon.svg` | Service icon, referenced as `icon = ./icon.svg` |

Rename `example` (directory, module names and the `plugin.name "example"` calls) to your service and replace the internals. Everything else follows from `name`.

## How Neo loads a plugin

You do not write this wiring; it explains what your plugin can rely on.

1. **Inputs.** The homeserver config (`templates/homeserver/modules/inputs.nix`) turns each entry `core.plugins.<id>` into an input `plugin-<id>` with:

   ```nix
   plugin-portrait = {
     url = "<your URL>";
     inputs.neo.follows = "neo";
     inputs.nixpkgs.follows = "nixpkgs";
     inputs.flake-file.follows = "flake-file";
   };
   ```

   So your plugin always builds against the host's Neo and nixpkgs (currently `nixos-26.05`), not the ones in your own `flake.lock`. Keep your plugin's inputs named `neo`, `nixpkgs` and `flake-file`. Other inputs you add (for example `declarr`, `nix2container`) are locked in the host's `flake.lock` as transitive inputs; give them `inputs.nixpkgs.follows = "nixpkgs"` when they accept it.

2. **Modules.** `templates/homeserver/modules/nixos.nix` imports, for every entry, `inputs.plugin-<id>.neoPlugin.module { id; url; prefix; }` next to Neo's own modules, into both `nixosConfigurations.neo` and `nixosConfigurations.vm`. A plugin without `neoPlugin` (written before prefixes existed) is imported through its `nixosModules.default`, without a prefix.

3. **Export.** `inputs.neo.flakeModules.plugin` (imported in `modules/imports.nix`) collects every `neoPlugin.modules.<name>` you define. `neoPlugin.module` wraps each one so it gets the `plugin` argument of that copy. It also exports `nixosModules.default` for older Neo hosts, and the check `neo-plugin-instances`. Modules still under `flake.modules.nixos.*` are imported as they are, and then the plugin cannot take a prefix.

4. **`lib`.** The host passes `specialArgs.lib = inputs.neo.lib`: nixpkgs' `lib` extended with `lib.neo`. Inside every NixOS module, `lib.neo.*` is available. In the outer flake-parts function (the `{inputs, ...}:` at the top of a file), `lib` is plain nixpkgs lib; use `inputs.neo.lib.neo` there if you need it.

5. **Settings.** The host sets `neo = builtins.fromTOML settings.toml`, so `[services.myapp]` in `settings.toml` is `config.neo.services.myapp`. Every key in `settings.toml` must be a declared option. If you rename or remove an option, installs that set it fail to evaluate until the operator removes the key; avoid breaking renames.

## Layout rules

- **Dendritic layout.** `import-tree` loads every `.nix` file under `modules/` as a flake-parts module. Each file defines one or more `neoPlugin.modules.<unique-name>` entries. Names must be unique within the plugin: use `<service>`, `<service>-option`, `<service>-swag`, `<service>-skills`.
- **No sibling imports.** Never write `imports = [ ./option.nix ];`. Every file is already imported, and a file imported by path is loaded only once even when the plugin is added twice. Share values through options (`config.neo.services.<name>.*`) instead.
- **Names through `plugin.name`.** Every name your modules put into the system comes from `plugin.name "<service>"` (see [Prefixes](#prefixes-add-a-plugin-twice)).
- **Flakes only see git-tracked files.** `git add` every new file before building, including icons and scripts. An untracked file is silently missing.
- **Files and directories whose name starts with `_`** are skipped by `import-tree`. Put non-module Nix files (data, helper functions you `import`) there, or outside `modules/`.
- **Option names** are camelCase under `neo.services.<name>` (like the built-in services), with plain-string descriptions. The service name itself may contain `-` (`portrait-server`).
- **Format** with `nix fmt` (alejandra).

A file looks like this:

```nix
# modules/services/myapp/default.nix
{inputs, withSystem, ...}: {          # flake-parts arguments (your plugin's inputs)
  flake.modules.nixos.myapp = {config, lib, pkgs, ...}: {
    # a normal NixOS module; lib.neo is available here
  };
}
```

## Prefixes: add a plugin twice

An operator can add your plugin twice, for example `main` and a `dev` branch, with a prefix on the second copy ([Add the same plugin twice](#add-the-same-plugin-twice)). The NixOS module system has no namespaces, so this only works if your modules never write a service name as a literal. Each module gets a `plugin` argument for its copy:

| Attribute | Value |
|-----------|-------|
| `plugin.name "x"` | `x`, or `<prefix>-x` |
| `plugin.prefix` | `""` or the prefix, e.g. `dev` |
| `plugin.label` | `""` or ` (dev)`, for human titles |
| `plugin.id` | The operator's id for this copy (`portrait-dev`) |
| `plugin.url` | The flake URL of this copy |

```nix
{...}: {
  neoPlugin.modules.myapp = {config, lib, plugin, ...}: let
    name = plugin.name "myapp";          # "myapp" or "dev-myapp"
    cfg = config.neo.services.${name};
  in {
    config = lib.mkIf cfg.enabled {
      virtualisation.oci-containers.containers.${name} = { … };
      systemd.services."docker-${name}".preStart = …;
    };
  };
}
```

Name with `plugin.name`: the `neo.services` key, containers and sidecars (`"${name}-db"`), systemd units, appdata, the default subdomain, users and groups, secret templates, guide ids and `service:` links, and the hostnames your containers use to reach each other (`http://${plugin.name "prowlarr"}:9696`). Names of Neo core services (`swag`, `tinyauth`) stay as they are.

Ports two copies would share must be settings, so the operator can give the second copy another one: host ports, and container ports of services that can run behind the VPN (all VPN-routed containers share gluetun's network namespace). Ports on the `internal` Docker network do not clash; each copy has its own container.

`nix flake check` runs `neo-plugin-instances`: it evaluates Neo with your plugin once, then twice with the prefixes `neoa` and `neob`, and fails with the list of containers, units, users and services that are the same in every copy. It also fails when a container runs an image the plugin builds itself (tagged with a Nix hash) under a name without the prefix: copies sharing an image name remove each other's image when they clean up old builds. Images pulled from a registry may be shared.

**Porting an existing plugin:** import `inputs.neo.flakeModules.plugin` in `modules/imports.nix`, drop your own `nixosModules.default` from `modules/nixos.nix`, rename `flake.modules.nixos.<x>` to `neoPlugin.modules.<x>`, add `plugin` to each module's arguments and replace the literals with `name`. Existing installs keep working: with an empty prefix every name is what it was. A plugin that keeps any `flake.modules.nixos.*` module still loads, once, without a prefix.

## Development loop

**In a Neo checkout with the QEMU VM** (fastest):

1. In the Neo repo's `settings.toml`, add your plugin: `[core.plugins.my-plugin]` with `url = "git+file:/home/you/projects/my-plugin"`. Add a second table with `prefix = "dev"` to try it twice.
2. Enable your service: `[services.myapp] enabled = true`.
3. `git add` new files in the plugin.
4. `just launch` (rebuilds the config from scratch, so the plugin is re-locked every time), then run the [checks](#verify-on-the-host) with `just exec '…'`.

**On a real homeserver:**

1. Put the plugin on the server (or push it to a forge) and add it under **Settings → System → Plugins**.
2. After changing the plugin, commit or `git add`, then **Update** (or `neo update`) so the lock picks up the new revision, then **Activate**.

`neo build` builds without switching, which is a quick way to see evaluation errors. `nix flake check` in the plugin runs `neo-plugin-instances`, which evaluates Neo with your plugin once and twice and lists names that miss `plugin.name`.

## Turn your project into a Neo plugin (checklist)

Work through every item. Each one exists because it is easy to forget.

**Flake**

- [ ] Started from `nix flake init -t github:madebydamo/neo#plugin`; inputs `neo`, `nixpkgs`, `flake-file` kept.
- [ ] Extra inputs added in `modules/inputs.nix` (`flake-file.inputs.<name>`), with `inputs.nixpkgs.follows = "nixpkgs"` where possible; `nix run .#write-flake` re-run; `flake.lock` updated.
- [ ] Packages you build live in `perSystem.packages` and are reached from NixOS modules with `withSystem pkgs.stdenv.hostPlatform.system ({config, ...}: config.packages.<name>)`, or built directly in the module from `pkgs`.
- [ ] `modules/imports.nix` imports `inputs.neo.flakeModules.plugin`; modules are `neoPlugin.modules.<name>`, not `flake.modules.nixos.<name>`.
- [ ] Every new file is `git add`ed. No `imports = [ ./sibling.nix ]`. Module names are unique.
- [ ] Every module takes `plugin` and names the service, containers, units, appdata, subdomain, secrets and the hostnames of its own containers with `plugin.name "…"`. `nix flake check` passes (`neo-plugin-instances`).

**`option.nix`** ([details](#optionnix))

- [ ] `options.neo.services.${name} = mkOption { type = types.submodule {...}; default = {}; }` with `name = plugin.name "<service>"`.
- [ ] `enabled = mkEnableOption "…" {rank = 0;}` (Neo's `mkEnableOption` takes a second attrset argument).
- [ ] Service-specific options at ranks 10–89; internal ports as `internal = true`.
- [ ] `// lib.neo.mkReverseProxyOptions { subdomain = name; auth.publicPaths = [ … ]; }` if it has a web UI (or `plugin.name "<short>"` for a shorter subdomain). It must not collide with another service.
- [ ] `// lib.neo.mkContainerDefinitions { <container> = "<image:tag>"; extraUnits = [ … ]; }` for OCI services, listing every non-docker unit in `extraUnits`; or `// lib.neo.mkSystemdUnits [ … ]` for native services. Every unit the service runs is listed.
- [ ] `// lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/<name>"` if it stores anything.
- [ ] `// lib.neo.mkServiceMeta { category; icon; description; projectUrl; githubUrl; releaseUrl; }`.
- [ ] `// lib.neo.mkSkillOptions {}` if you ship a `skills.nix`.
- [ ] `// lib.neo.mkVpnOptions { … }` if containers may route through the VPN.
- [ ] Secrets (passwords, tokens, API keys) use `lib.neo.mkSecretOption` (same arguments as `mkOption`, e.g. `type = types.nullOr types.str; default = null; helper = lib.neo.helpers.randomToken;`); never a real secret as a default.
- [ ] Presentation metadata where useful: `label`, `ui.group`, `visibleWhen`, `choices`, `choiceLabels`.

**`default.nix`** ([details](#defaultnix))

- [ ] Everything inside `config = lib.mkIf cfg.enabled { … }`.
- [ ] Appdata directories created with `lib.neo.mkEnsureDirs config [ … ]` in the container's `preStart` (or tmpfiles for native units).
- [ ] Containers: `image = cfg.containers.<c>`, `autoStart = true`, `networks = ["internal"]`.
- [ ] `PUID`/`PGID = toString config.neo.core.uid/gid`, `TZ = config.neo.core.timeZone` (if the image supports them).
- [ ] All state under `${config.neo.core.volumes.appdata}/<name>`. Media and documents from `config.neo.core.volumes.media` / `.documents`.
- [ ] No host ports for web UIs; SWAG reaches containers over `internal`. Only publish non-HTTP protocol ports.
- [ ] Post-start configuration in `lib.neo.mkSetupService`, also listed in `extraUnits`.
- [ ] `assertions` for required secrets and invalid combinations.
- [ ] Secrets never in `environment`, `/etc` text or a script: `neo.core.secrets.containerEnv.<container>` or `neo.core.secrets.templates` ([details](#secrets)).
- [ ] Native NixOS services: `User`/`Group = "homeserver"`, data paths in appdata, `ReadWritePaths`, tmpfiles `d` rule, no `DynamicUser`/`StateDirectory`, nothing in `/var/lib/<name>`.
- [ ] Host listeners bind `127.0.0.1` and use `lib.neo.mkDockerToLocalhostForward port`.
- [ ] Privileged commands via `lib.neo.mkSudoExtraRules`; git access via `neo.core.git.users`.

**`swag.nix`** ([details](#swagnix))

- [ ] `config.neo.services.<name>.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf { inherit config cfg; upstream = "<container>"; port = <port>; });`
- [ ] Hand-written blocks include `listen-https.conf`, `ssl.conf`, `geo-access.conf`, `proxy.conf`, `${lib.neo.authBlock config cfg}` in the location and `${lib.neo.authLocations config cfg}` in the server.
- [ ] No re-set `Upgrade`/`Connection` headers or `proxy_*_timeout` (already in `proxy.conf`).
- [ ] Health, API, share and app endpoints that clients call without a browser login are in `auth.publicPaths`.

**`skills.nix`** ([details](#skillsnix))

- [ ] `config.neo.services.<name>.skill.conf = lib.neo.mkServiceSkill { service; inherit cfg domain; description; body; };`
- [ ] The body names only settings keys that exist (`services.<name>.<option>`), and real units, paths and URLs.

**Verify** ([details](#verify-on-the-host))

- [ ] Units active, internal curl from SWAG works, public health path 200, UI 302 to tinyauth, no SWAG `emerg`.
- [ ] The service appears on the Services grid with icon, category, description and a plugin badge; its status shows its units; **Clear appdata** removes the right directory.

## Service files in detail

The examples use a service `myapp`: one container `myapp` on port 8080, a setup step, a secret, and a public health endpoint.

### option.nix

```nix
# modules/services/myapp/option.nix
{...}: {
  neoPlugin.modules.myapp-option = {config, lib, plugin, ...}:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      name = plugin.name "myapp";
    in {
      options.neo.services.${name} = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "MyApp notes server${plugin.label}" {rank = 0;};

              port = mkOption {
                type = types.port;
                internal = true;
                default = 8080;
                description = "Port MyApp listens on inside the container";
              };

              # Secret: kept out of settings.toml once the operator encrypts.
              adminPassword = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = "Password of the MyApp admin account";
                helper = lib.neo.helpers.randomToken;
              };

              allowSignup = mkOption {
                type = types.bool;
                default = false;
                rank = 20;
                description = "Let anyone who can reach the site create an account";
                ui = lib.neo.ui.mkUi {label = "Allow sign-up";};
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = name;
              auth.publicPaths = ["^/api/health$" "^/api/"];
            }
            // lib.neo.mkContainerDefinitions {
              ${name} = "ghcr.io/example/myapp:1.4";
              extraUnits = ["${name}-setup"];
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/${name}"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = ./icon.svg;
              description = ''
                MyApp is a notes server. Its data lives in Neo appdata.
              '';
              projectUrl = "https://myapp.example";
              githubUrl = "https://github.com/example/myapp";
              releaseUrl = "https://github.com/example/myapp/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "MyApp notes server";
      };
    };
}
```

Notes:

- `name` is `myapp`, or `dev-myapp` when an operator adds the plugin a second time with the prefix `dev`. Use it for every name the plugin puts into the system, never the literal (see [Prefixes](#prefixes-add-a-plugin-twice)).
- `with {inherit (lib.neo) mkOption mkEnableOption;}` swaps in Neo's wrappers, which accept `rank`, `helper` and `ui`. `lib.mkOption { … } // { rank = 10; }` is equivalent.
- `mkReverseProxyOptions` adds `subdomain` (100), `ingress` (105), `auth` (120), `customDomains` (130) and the internal `proxyConf`. `auth.enabled` defaults to true. Pass `auth.enabled = false` for a public site, or `auth.available = false` for apps that must never sit behind tinyauth (the auth option is then hidden).
- `mkContainerDefinitions` adds `containers.<name>` image options (rank 300, Advanced group) and the internal `systemdUnits` (`docker-<name>` for each container, plus `extraUnits` and `timerUnits`). Do not also call `mkSystemdUnits`.
- Pin a real tag when the app has breaking upgrades. The docker updater pulls the configured tag.
- Service categories in use: Core, Network, Security, Media, Files, Monitoring, Utilities, AI, Other.

### default.nix

```nix
# modules/services/myapp/default.nix
{...}: {
  neoPlugin.modules.myapp = {config, lib, pkgs, plugin, ...}: let
    name = plugin.name "myapp";
    cfg = config.neo.services.${name};
    appdata = "${config.neo.core.volumes.appdata}/${name}";
    domain = config.neo.services.swag.domain;
  in {
    config = lib.mkIf cfg.enabled {
      assertions = [
        {
          assertion = cfg.adminPassword != null && cfg.adminPassword != "";
          message = "neo.services.${name}: set adminPassword (use the Generate helper).";
        }
      ];

      systemd.services."docker-${name}".preStart = lib.neo.mkEnsureDirs config [
        appdata
        "${appdata}/data"
      ];

      virtualisation.oci-containers.containers.${name} = {
        image = cfg.containers.${name};
        autoStart = true;
        environment = {
          PUID = toString config.neo.core.uid;
          PGID = toString config.neo.core.gid;
          TZ = config.neo.core.timeZone;
          MYAPP_PUBLIC_URL = "https://${cfg.subdomain}.${domain}";
          MYAPP_SIGNUP = lib.boolToString cfg.allowSignup;
        };
        volumes = [
          "${appdata}/data:/data"
          "${config.neo.core.volumes.documents}:/documents"
        ];
        networks = ["internal"];
      };

      # Secrets reach the container through a rendered env file, never `environment`.
      neo.core.secrets.containerEnv.${name} = {
        MYAPP_ADMIN_PASSWORD = cfg.adminPassword;
      };

      # Post-start configuration. Also listed in extraUnits (option.nix).
      systemd.services."${name}-setup" = lib.neo.mkSetupService {
        inherit pkgs;
        name = "${name}-setup";
        description = "Configure MyApp${plugin.label} after first start";
        containers = [name];
        retryInterval = 15;
        path = [config.virtualisation.docker.package];
        script = ''
          docker exec ${name} myapp-cli set-admin --password-env MYAPP_ADMIN_PASSWORD
        '';
      };
    };
  };
}
```

Notes:

- **Network.** The `internal` Docker network is created by SWAG. Containers reach each other, and SWAG reaches them, by container name. Container names are global Docker names: name sidecars after the service (`"${name}-db"`, `"${name}-redis"`).
- **Ownership.** `mkEnsureDirs` creates missing directories owned by `neo.core.uid:gid` (the `homeserver` user). Pass `{ dirPath; user; group; mode; }` for images that run as a fixed uid (for example postgres `999`).
- **Appdata.** Everything mutable lives under `${appdata}`. That is what **Clear appdata** removes and what snapshots and backups cover. Do not write to `/var/lib/<name>`.
- **Ports.** Do not publish web UI ports (`ports = [ … ]`): that bypasses SWAG, tinyauth, ingress and the NixOS firewall (Docker publishes ports through its own iptables rules). Publish only protocol ports clients need directly, like Syncthing's `22000`.
- **Setup units.** `mkSetupService` retries the script until it exits 0, stays "active (exited)" after success, and runs again when the container restarts. Use `blocking = true` only when another unit must wait for it.
- **Timers.** For scheduled jobs pass `timerUnits = [ "myapp-sync" ];` to `mkContainerDefinitions` (or `mkSystemdUnits` + `mkSystemdTimers` without containers). The service target arms the timer and does not run the job on start.
- **Secrets.** Declare them with `lib.neo.mkSecretOption`, offer a Generate helper, assert that they are set, and hand them to the service through `neo.core.secrets` (see [Secrets](#secrets)). Never through `environment`: that lands in the Nix store, and on an encrypted config the build fails.
- **Domain.** `config.neo.services.swag.domain` can be `null` on a fresh install; guard it (`or "localhost"`) where a null would break evaluation.

### swag.nix

```nix
# modules/services/myapp/swag.nix
{...}: {
  neoPlugin.modules.myapp-swag = {config, lib, plugin, ...}: let
    name = plugin.name "myapp";
    cfg = config.neo.services.${name};
  in {
    config.neo.services.${name}.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = name;
      port = cfg.port;
    });
  };
}
```

SWAG writes each enabled service's `proxyConf` to `proxy-confs/<subdomain>.subdomain.conf`, gets certificates for the subdomain and `customDomains`, and Pi-hole / split DNS serve the names locally.

If you need more than one `location`, write the block yourself. Keep the includes:

```nix
config.neo.services.${name}.proxyConf = lib.mkDefault ''
  server {
    include /config/nginx/listen-https.conf;
    http2 on;
    server_name ${cfg.subdomain}.*;
    include /config/nginx/ssl.conf;
    include /config/nginx/geo-access.conf;
    client_max_body_size 0;

    location / {
      include /config/nginx/proxy.conf;
      include /config/nginx/resolver.conf;
      set $upstream_app ${name};
      set $upstream_port ${toString cfg.port};
      set $upstream_proto http;
      proxy_pass $upstream_proto://$upstream_app:$upstream_port;
      ${lib.neo.authBlock config cfg}
    }
    ${lib.neo.authLocations config cfg}
  }
'';
```

| Include | Why |
|---------|-----|
| `listen-https.conf` | LAN/Tailscale listener and the PROXY-protocol listener for public traffic; applies the **ingress** setting ("Reachable from") and the Neo 404 page |
| `ssl.conf` | Certificates |
| `geo-access.conf` | Country / IP allow and block lists from the SWAG settings |
| `proxy.conf` | Forwarding headers, WebSocket upgrade, timeouts |
| `resolver.conf` | Needed when `proxy_pass` uses a variable (container names resolve at request time, so SWAG starts even if the container is down) |
| `authBlock` / `authLocations` | tinyauth gate; both render empty when `auth.enabled` is false or tinyauth is off |

**SWAG traps.** `proxy.conf` already sets `Upgrade`, `Connection` and the `proxy_*_timeout`s. Setting them again breaks WebSockets (426) or produces duplicate directives, which makes nginx refuse **all** vhosts. Do not add `host.docker.internal` to SWAG; it is already there.

**tinyauth.** With auth on, the UI answers `302` to the tinyauth login. Anything a client calls without a browser session (health checks, mobile apps, API tokens, share links, CalDAV, webhooks) needs a regex in `auth.publicPaths`. Those paths are then protected only by the app's own auth.

**Ingress.** `ingress` (`local`, `tailscale`, `web`) is per service and enforced by `listen-https.conf`. You do not need to do anything, as long as hand-written blocks include it.

### skills.nix

Hermes, Neo's AI assistant, gets one skill per enabled service that sets `skill.conf`.

```nix
# modules/services/myapp/skills.nix
{...}: {
  neoPlugin.modules.myapp-skills = {config, lib, plugin, ...}: let
    name = plugin.name "myapp";
    cfg = config.neo.services.${name};
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.${name}.skill.conf = lib.neo.mkServiceSkill {
      service = name;
      inherit cfg domain;
      description = "MyApp notes server";
      tags = ["neo" "myapp" "notes"];
      body = ''
        ## When to Use
        Questions about notes stored in MyApp.

        ## Procedures
        1. systemctl status docker-${name} ${name}-setup
        2. docker logs ${name} --tail 100

        ## Credentials
        - Admin password: setting `services.${name}.adminPassword`
      '';
    };
  };
}
```

`mkServiceSkill` adds an "About", a live-config table (URL, units, containers, appdata, auth, public paths) and an ops cheatsheet from `cfg`. Only mention settings keys that exist. Hermes collects skills only from enabled services with `skill.enabled = true`.

## Other service shapes

### Native NixOS module instead of a container

Follow `nix/services/vikunja/` in the Neo repo:

```nix
config = lib.mkIf cfg.enabled (lib.mkMerge [
  (lib.neo.mkDockerToLocalhostForward cfg.port)   # SWAG -> host.docker.internal -> 127.0.0.1
  {
    services.myapp = {
      enable = true;
      address = "127.0.0.1";
      port = cfg.port;
      dataDir = appdata;
    };
    systemd.tmpfiles.rules = ["d ${appdata} 0755 homeserver homeserver -"];
    systemd.services.myapp.serviceConfig = {
      DynamicUser = lib.mkForce false;
      User = "homeserver";
      Group = "homeserver";
      StateDirectory = lib.mkForce [];
      ReadWritePaths = [appdata];
    };
  }
]);
```

In `option.nix` use `// lib.neo.mkSystemdUnits ["myapp"]` instead of `mkContainerDefinitions`. In `swag.nix`:

```nix
lib.neo.mkSubdomainProxyConf {
  inherit config cfg;
  proxyPass = "http://host.docker.internal:${toString cfg.port}/";
}
```

Bind to `127.0.0.1`, not `0.0.0.0`, so the port is not exposed on the LAN.

### Static site

Build the site as a derivation and serve it with the host nginx on loopback, the same way as above: `services.nginx.virtualHosts.<name>` listening on `127.0.0.1:<port>`, `lib.neo.mkDockerToLocalhostForward <port>`, `lib.neo.mkSystemdUnits ["nginx"]`, and `proxyPass = "http://host.docker.internal:<port>/"`. The host nginx is shared between static-site plugins, so pick a port nobody else uses. Neo keeps `nginx` out of per-service targets so stopping one site does not stop the others.

### Image built by the plugin

See [portrait](https://github.com/madebydamo/portrait): build an OCI image with `nix2container` in `perSystem.packages`, load it with a oneshot unit `"${name}-image"` before `docker-${name}`, straight under the copy's own name (`${image.copyTo}/bin/copy-to docker-daemon:${name}:${image.imageTag}`), so each copy of the plugin cleans up only its own images. Then `mkContainerDefinitions { ${name} = "${name}:${builtins.unsafeDiscardStringContext image.imageTag}"; extraUnits = ["${name}-image"]; }`.

### Host packages or extra inputs

Use your plugin's inputs inside a NixOS module through the flake-parts arguments:

```nix
{inputs, ...}: {
  neoPlugin.modules.myapp = {pkgs, ...}: let
    myPkg = inputs.mytool.packages.${pkgs.stdenv.hostPlatform.system}.default;
  in {
    # …
  };
}
```

### System-level additions

A plugin does not have to define a service. Any `neoPlugin.modules.<name>` is imported, so disks, packages or kernel tweaks are plain NixOS config. It will not show on the Services grid. A module that is not named per instance (a shared package, a kernel setting) is imported once per copy of the plugin, so keep it to definitions that merge (no option declarations), or the plugin can only be added once.

## Verify on the host

On the VM, prefix each command with `just exec '…'` from the Neo repo. On a real server, run them over SSH. `<domain>` is `services.swag.domain`.

```bash
systemctl is-active docker-myapp myapp-setup docker-swag docker-tinyauth
journalctl -b -u docker-myapp --no-pager | tail -50
docker exec swag curl -sS -m 10 http://myapp:8080/api/health           # internal path
docker exec swag cat /config/nginx/proxy-confs/myapp.subdomain.conf     # rendered vhost
docker logs swag 2>&1 | grep -i emerg                                   # must be empty
curl -sk -m 20 -w "\n%{http_code}\n" "https://myapp.<domain>/api/health"  # 200 (publicPaths)
curl -sk -m 20 -D - -o /dev/null "https://myapp.<domain>/"                # 302 to tinyauth
```

**Pass:** units active, internal curl OK, public health 200, UI 302 to tinyauth, no SWAG `emerg`. In the web UI the service shows its units as running, and setup units show as done.

## `config.neo` values a plugin reads

| Value | Type, default | Use |
|-------|---------------|-----|
| `config.neo.core.volumes.root` | str, `/var/neo` | Root of all Neo volumes |
| `config.neo.core.volumes.appdata` | str, `<root>/DATA/AppData` | Per-service state: `<appdata>/<name>` |
| `config.neo.core.volumes.data` | str, `<root>/DATA` | Shared data root |
| `config.neo.core.volumes.media` | str, `<root>/DATA/Media` | Media library |
| `config.neo.core.volumes.documents` | str, `<root>/DATA/Documents` | Documents |
| `config.neo.core.uid` / `gid` | int, `1000` | uid/gid of the `homeserver` user; `PUID`/`PGID`, file ownership |
| `config.neo.core.timeZone` | str, `Europe/Zurich` | `TZ` |
| `config.neo.core.hostname` | str, `nixos` | Hostname |
| `config.neo.core.git.users` | list of str, internal | Append a local user that needs the machine git key and tokens |
| `config.neo.services.swag.domain` | null or str, `null` | Primary domain; public URL is `https://<subdomain>.<domain>` |
| `config.neo.services.tinyauth.enabled` | bool | Whether edge auth runs (`authBlock` already checks it) |
| `config.neo.services.<name>.*` | | Your own options, plus `subdomain`, `auth`, `containers`, `appdata`, `systemdUnits` from the helpers. Other services' options can be read the same way (guard with `or`, the service may be off) |

The `homeserver` user and group own all appdata. It is in the `docker` group.

## `lib.neo` reference

`inputs.neo.lib` is nixpkgs' `lib` extended with `lib.neo`. Every file in `nix/lib/` contributes one `libExtensions.<name>.neo` attrset; `nix/lib/default.nix` deep-merges them. The list below is complete. Items marked **internal** are used by Neo's own modules; plugins rarely need them.

### Option wrappers

| Function | Arguments | Returns |
|----------|-----------|---------|
| `mkOption` | Same as `lib.mkOption`, plus optional `rank` (int), `helper` (from `mkHelper`), `ui` (from `ui.mkUi`) | Option with the extra keys attached |
| `mkSecretOption` | Same as `mkOption` (`type` defaults to `types.str`) | Option tagged `secret = true`: masked in the web UI, moved to `secrets.yaml` / `secrets.sops.yaml` by the operator. See [Secrets](#secrets) |
| `mkEnableOption` | `description: args:`, both required; `args` is an attrset like `{rank = 0; ui = …;}` (may be `{}`) | `lib.mkEnableOption description // args` |

```nix
enabled = lib.neo.mkEnableOption "MyApp" {rank = 0;};
token = lib.neo.mkOption {
  type = lib.types.str;
  default = "";
  description = "API token";
  rank = 10;
  helper = lib.neo.helpers.randomToken;
};
```

### Option sets for a service submodule

Merge these with `//` into `neo.services.<name>`'s `options`.

| Function | Arguments (defaults) | Adds |
|----------|---------------------|------|
| `mkReverseProxyOptions` | `{ subdomain ? null, auth ? null, customDomains ? [] }`. `auth` is `{ enabled ? true, publicPaths ? [], available ? true }` | `subdomain` (rank 100), `ingress` (105, `["local" "tailscale" "web"]`), `auth.{enabled,publicPaths}` (120), `customDomains` (130), internal `proxyConf`. All in the Access group. `available = false` hides `auth` and defaults it off |
| `mkVpnOptions` | `{ containers ? [], internalContainers ? [], ports ? [], networks ? [], enabled ? false }` | `vpn.enabled` (110, Access group) plus internal `vpn.containers`, `internalContainers`, `ports`, `networks`. When enabled, `containers` join gluetun's network; `ports` must be unique across VPN-routed services. `networks` is also applied to `containers` and `internalContainers` |
| `mkContainerDefinitions` | Attrset `{ <container> = "<image>"; … }` plus optional `extraUnits ? []`, `timerUnits ? []`, `rank ? 300` | `containers.<container>` image options (Advanced group), internal `systemdUnits` (`docker-<container>` + extras + timers), internal `systemdTimers` |
| `mkSystemdUnits` | List of unit names without `.service` | Internal `systemdUnits`. For services without containers |
| `mkSystemdTimers` | List of unit names | Internal `systemdTimers`: the target arms `<unit>.timer` instead of starting the job. Units must also be in `systemdUnits` |
| `mkAppdata` | Absolute path, usually `"${config.neo.core.volumes.appdata}/<name>"` | Internal `appdata` used by Clear appdata, snapshots and skills |
| `mkServiceMeta` | `{ icon ? null, description ? "", projectUrl ? null, githubUrl ? null, releaseUrl ? null, screenshots ? [], rank ? null, iframeCompatible ? true, category ? "Other" }`. `icon` is a URL string or a path (`./icon.svg`, png, webp, ico); paths are served at `/service-icons/…`. `screenshots` is a list of `{ url, caption ? null }` | Internal `meta`, shown on the Services grid, navigator and option pane. `rank` sorts the grid; set `iframeCompatible = false` for apps that refuse to load in the navigator iframe |
| `mkSkillOptions` | `{ enabled ? true }` | `skill.enabled` (200, AI assistant group) and internal `skill.conf`, set by `skills.nix` |
| `mkAdditionalMountPoints` | `{ rank ? 20, description ? … }` | `additionalMountPoints`: list of `{ localPath, containerPath }` for extra bind mounts |
| `mkSshConnectionOptions` | `{ rankBase ? 10, defaultSshKey ? "/home/homeserver/.ssh/id_ed25519", hostDescription, userDescription, sshKeyDescription, extraOptionsDescription }` (descriptions have defaults) | `host`, `user`, `sshKey`, `extraOptions` at ranks `rankBase` … `rankBase + 30` |

```nix
// lib.neo.mkAdditionalMountPoints {rank = 30;}
# default.nix:
volumes = ["${appdata}:/config"] ++ lib.neo.toOciBindMounts cfg.additionalMountPoints;
```

### Secrets

The operator decides where secret settings live (Settings → Secrets or `neo secrets split|enable`): inline in `settings.toml`, in a plain `secrets.yaml`, or encrypted in `secrets.sops.yaml`. A plugin works the same in all three if it follows these rules. Options declared with plain `mkOption` keep working, but their values always stay in `settings.toml` in plain text.

On an encrypted config, a secret option evaluates to a placeholder (`@NEO_SECRET:services/myapp/adminPassword@`). The real value is filled in only when the system activates, by root, into files under `/run/neo-secrets/rendered/`. The service code therefore passes the option on as a string and never inspects it:

- **Container environment:** `neo.core.secrets.containerEnv.<container> = { VAR = cfg.secret; };` renders an env file, adds it to the container's `environmentFiles`, and restarts `docker-<container>` when it changes.
- **Files:** `neo.core.secrets.templates."<name>" = { content = "…${cfg.secret}…"; owner; group; mode ? "0400"; restartUnits ? []; copies ? []; };` (or `env = { … }` for an env file). Read it from `config.neo.core.secrets.templates."<name>".path`; `copies` writes the same file to paths the app expects (for example in appdata). DynamicUser units take it with `LoadCredential`.
- **Derived values:** never hash or encode a secret in Nix. Use `lib.neo.secrets.sha256 s`, `lib.neo.secrets.base64 s` (computed when the system activates) and `lib.neo.secrets.jsonString s` (a quoted JSON string, for JSON or JS files).
- **SWAG lines with a secret** (an `Authorization` header, for example): `let snip = lib.neo.secrets.swagSnippet config "myapp-auth" "proxy_set_header Authorization \"Bearer ${cfg.token}\";"; in` put `${snip.include}` in the proxy conf and set `neo.core.secrets.templates = snip.templates;`.
- A required-secret assertion (`cfg.secret != null && cfg.secret != ""`) works unchanged: a placeholder is never empty.

The build fails if a placeholder would end up in a container or unit `environment` or in `/etc` text.

### Containers and units

| Function | Arguments | Returns |
|----------|-----------|---------|
| `toOciBindMounts` | List of `{ localPath, containerPath }` | List of `"host:container"` strings |
| `mkSetupService` | `{ pkgs, name, description, script, containers ? [], attachTo ? containers, after ? [], wants ? [], requires ? [], bindsTo ? [], partOf ? [], before ? [], wantedBy ? [], path ? [], environment ? {}, retryInterval ? 60, maxAttempts ? null, blocking ? false, serviceConfig ? {}, unitConfig ? {} }` | A `systemd.services.<name>` value. `After`/`Requires` on `docker-<c>` for each of `containers`; `docker-<c>` of `attachTo` wants it. `script` is one attempt under `bash -e`, retried every `retryInterval` s until exit 0 (`null` = one attempt, then `Restart=on-failure`); `maxAttempts` gives up. `RemainAfterExit` always; `Type=simple`, or `oneshot` with `blocking = true` |
| `isSetupService` | A `systemd.services` value | bool; **internal** |
| `setupUnitMarker` | | `"X-Neo-Setup"`, the unit key that marks setup units; **internal** |
| `getAllContainers` | `config` | List of `{ service, container, image, unit }` for enabled services whose `docker-<c>` is in `systemdUnits`; **internal** (docker updater) |
| `getEnabledServiceUnits` | `config` | List of `{ service, units }`; **internal** |
| `planServiceTargets` | `{ services, exclude ? [ "dnsmasq" "neo-web" "neo-bootstrap" "swag-patcher" "nginx" ] }` | `neo-<service>.target` plan; **internal** |

List every setup unit in the service's `systemdUnits` (`extraUnits` or `mkSystemdUnits`). An assertion fails evaluation for setup units that are not listed in an enabled service's `systemdUnits`, so define them inside `mkIf cfg.enabled`.

### Directories and files

| Function | Arguments | Returns |
|----------|-----------|---------|
| `mkEnsureDirs` | `config: dirs:`; each entry is a path string or `{ dirPath, mode ? "0755", user ? uid, group ? gid }` | Shell snippet that creates each missing directory with that owner and mode. Use in `preStart` or `system.activationScripts` |
| `mkActivationScriptForDir` | `config: { dirPath, mode ? "0755", user ? toString config.neo.core.uid, group ? toString config.neo.core.gid }` | Shell snippet for one directory (only acts if it does not exist) |
| `mkActivationScriptForFile` | `config: { filePath, content, mode ? "0644", user ? uid, group ? gid }` | Shell snippet that creates the parent dir and **overwrites** the file with `content` on every run |

```nix
systemd.services.docker-myapp.preStart = lib.concatStringsSep "\n" [
  (lib.neo.mkEnsureDirs config [
    "${appdata}/config"
    {dirPath = "${appdata}/db"; user = "999"; group = "999";}
  ])
  (lib.neo.mkActivationScriptForFile config {
    filePath = "${appdata}/config/settings.json";
    content = builtins.toJSON {port = cfg.port;};
  })
];
```

### Reverse proxy (SWAG, tinyauth)

| Function | Arguments | Returns |
|----------|-----------|---------|
| `mkSubdomainProxyConf` | `{ config, cfg, upstream ? null, port ? null, proto ? "http", proxyPass ? null, maxBodySize ? "0", auth ? true, includeResolver ? (proxyPass == null), geo ? true }`. Give `upstream` + `port`, or `proxyPass` (full URL). `maxBodySize = null` omits `client_max_body_size`; `"0"` is unlimited | Full nginx `server { … }` block for `<subdomain>.*` with listen, SSL, geo, `proxy.conf`, resolver and tinyauth |
| `authBlock` | `config: cfg:` | Location-level tinyauth include, or `""` when `cfg.auth.enabled` is false or tinyauth is off |
| `authLocations` | `config: cfg:` | Server-level `/tinyauth` include, or `""` |
| `getProxiedServices` | `config` | Enabled services with a non-null `subdomain` |
| `tinyauthLocationConf`, `mkTinyauthServerConf config` | | Contents of `tinyauth-location.conf` / `tinyauth-server.conf`; **internal** |
| `listenHttpsConf`, `httpsListenInclude`, `httpsProxyProtocolContainerPort` (8443), `errorPagesConf`, `realIpConf` | | Listener snippets behind `listen-https.conf`; **internal** |
| `geoAccessInclude`, `geoAccessConf`, `mkDbipConf geo` | | Geo allow/deny snippets; **internal** |
| `ingressVariants`, `mkIngressMapsConf { services, proxyProtocolPort ? 8443 }`, `ingressAccessConf` | | Per-service ingress maps; **internal** |

### Hermes skills

| Function | Arguments | Returns |
|----------|-----------|---------|
| `mkServiceSkill` | `{ service, cfg, description, body, domain ? null, name ? "neo-${service}", category ? "neo", tags ? ["neo" service], title ? null, includeCredentialsFooter ? true, references ? {}, scripts ? {} }` | Value for `skill.conf`: `{ name, description, category, tags, references, scripts, content }` |
| `mkSkillMd` | `{ name, description, body, category ? "neo", tags ? [], version ? "1.0.0", platforms ? ["linux"], requiresToolsets ? ["terminal"], cfg ? null, domain ? null, service ? null, title ? null, includeCredentialsFooter ? true, includeDerived ? true }` | Complete `SKILL.md` text (frontmatter, derived sections, body, footer) |
| `mkSkillDerivedSection` | `{ cfg ? null, domain ? null, service ? null }` | "About" + "Live config" + ops cheatsheet markdown |
| `skillPublicUrl` | `{ subdomain, domain }` | `https://<subdomain>.<domain>` or a placeholder |
| `skillCredentialsFooter` | | Generic credentials section (string) |
| `getSkillServices` | `config` | Enabled services with `skill.enabled` and a `skill.conf`; **internal** |

`references` and `scripts` map file names to contents, written as `references/<name>.md` and `scripts/<name>` next to the skill.

### Web UI metadata: `lib.neo.ui`

| Function / value | Arguments | Returns |
|------------------|-----------|---------|
| `ui.mkUi` | `{ group, label, summary, choiceLabels, visibleWhen, widget, choices, keysFrom, modes, save, emptyHint, entryLabel, choiceEmptyHint, catalog, oauth }`, all optional | `ui` attrset (null fields dropped) for `mkOption { ui = …; }` |
| `ui.mkGroup` | `{ id, label, description ? null, icon ? null, rank ? 100 }`; `icon` is a path or URL | Collapsible section; the option and everything below it render there |
| `ui.groups.access` / `assistant` / `advanced` | | Built-in groups (ranks 100 / 200 / 300) used by the generated option sets |
| `ui.icons.<name>` | | Path to a built-in SVG: `camera`, `cpu`, `folder`, `git`, `globe`, `key`, `lock`, `mail`, `map`, `shield`, `sparkles`, `wrench` |
| `ui.mkKeysFrom` | `{ option, extract ? "identity" }`; `extract` is `"identity"` or `"beforeColon"` | Makes an attrsOf's keys follow another sibling option |
| `ui.mkMode` | `{ id, label, active ? [], listLabel ? null, hintEmpty ? null, hintFilled ? null, badge ? null }` | One mode for `modes` (the `exclusiveListPair` widget) |
| `ui.mkSave` | `{ pruneEmptyEntries ? false, omitIfEmpty ? false }` | Save rules for `save` |
| `ui.mkOauth` | `{ script, runAs ? null, env ? {} }` | OAuth action for the `providerAuth` widget |

`mkUi` fields:

| Field | Meaning |
|-------|---------|
| `group` | Section (`ui.groups.*` or `ui.mkGroup`). Ungrouped options render in the always-open General section |
| `label` | Display name (default: humanized option name) |
| `summary = true` | Show the value as a chip on the collapsed group header |
| `choiceLabels` | Display names for enum / choice values, e.g. `{ web = "Internet"; }` |
| `visibleWhen` | `"enabled"` hides the field while that sibling bool is false; `"type=entra"` hides it unless the sibling equals the value |
| `choices` | On a `listOf str`: a list (`["a" "b"]`) for a checkbox grid, or a provider name. Provider today: `"authApps"` (enabled services with tinyauth auth) |
| `keysFrom` | From `ui.mkKeysFrom` |
| `widget` | `exclusiveListPair`, `pluginList`, `primaryItemList`, `providerAuth`, `proxyRouteList` (see [AGENTS.md](../AGENTS.md#uiwidget-composite-editors)) |
| `emptyHint`, `entryLabel`, `choiceEmptyHint`, `modes`, `save`, `catalog`, `oauth` | Widget-specific inputs |

Rank bands for the top level of `neo.services.<name>` (siblings only; inside a submodule start again at 0, 10, 20, …):

| Rank | Option |
|------|--------|
| 0 | `enabled` |
| 10–89 | Service-specific |
| 100 / 105 / 120 / 130 | `subdomain` / `ingress` / `auth` / `customDomains` |
| 110 | `vpn` |
| 200 | `skill` |
| 300 | `containers` |

Do not add per-service code to the web UI. If the generic form is not enough, a new widget goes into Neo itself (see AGENTS.md).

### Fill-assist helpers

| Function / value | Arguments | Returns |
|------------------|-----------|---------|
| `mkHelper` | `{ id, kind, script, label ? "Generate", description ? "", apply ? "set", inputs ? [] }`. `kind` is `"button"` (no inputs) or `"form"` (needs `inputs`: list of `{ name, type, label, required, placeholder }`, `type` e.g. `"str"`, `"password"`). `apply` is `"set"` or `"append"`. `script` is a path to a bash script that reads the form as JSON on stdin and prints the value on stdout | Helper for `mkOption { helper = …; }` |
| `helpers.randomToken` | | Button: 32 random bytes as hex |
| `helpers.randomToken32Hex` | | Button: 16 random bytes as 32 hex characters |
| `helpers.bcryptUser` | | Form (username, password): `user:bcrypt-hash` line |
| `helpers.mkpasswdSha512` | | Form (password): SHA-512 crypt hash |

### Host access: firewall, sudo, SSH, git

| Function / value | Arguments | Returns |
|------------------|-----------|---------|
| `mkDockerToLocalhostForward` | A port or list of ports | NixOS config (`mkIf`) with iptables DNAT so containers reach `host.docker.internal:<port>` on a service bound to `127.0.0.1`. Merge with `lib.mkMerge` |
| `mkSudoExtraRules` | `{ users ? [], groups ? [], runAs ? null, commands ? [], all ? false, options ? ["NOPASSWD" "SETENV"] }`. Each command is `{ package, name, options?, pname? }` or `{ command, options? }`. `runAs` is a string | Value for `security.sudo.extraRules` |
| `mkSudoCommand` | `{ package, name, options ? defaultSudoCommandOptions, pname ? package.pname }` | The four path forms sudo may see for one binary |
| `defaultSudoCommandOptions` | | `["NOPASSWD" "SETENV"]` |
| `mkSshKeyScript` | `pkgs: { name, path, owner, group, comment, mode ? "600", dirMode ? "700", requireDir ? null }` | Package with a `<name> [ensure\|rotate]` script that creates an ed25519 key |
| `defaultHomeserverSshKey` | | `/home/homeserver/.ssh/id_ed25519` |
| `gitSshKey` | | `/var/lib/neo/git/id_ed25519` (machine git key) |

```nix
security.sudo.extraRules = lib.neo.mkSudoExtraRules {
  users = ["homeserver"];
  commands = [{package = pkgs.systemd; name = "systemctl";}];
};

# A local user that clones or pushes with the machine git identity:
neo.core.git.users = ["myapp"];
```

Never hand-roll sudo path lists or per-service SSH/git config. Host services that must be reachable from outside need `networking.firewall.allowedTCPPorts` / `allowedUDPPorts`; web traffic always goes through SWAG instead.

### DNS, VPN and other readers

| Function | Arguments | Returns |
|----------|-----------|---------|
| `getVpnServices` | `config` | Enabled services with `vpn.enabled` |
| `getDockerNetworkServices` | `config` | Enabled services with non-empty `vpn.networks` |
| `localDnsNames` | `{ domain, onlySubdomains ? true, subdomains ? [], customDomains ? [], proxyPassDomains ? [] }` | FQDNs served by local DNS; **internal** |
| `localDnsNamesFromConfig` | `config` | Same, from the current config; **internal** |
| `splitDnsActive` | `config` | bool: Tailscale split DNS is on |
| `privateHostnameDnsActive` | `config` | bool: service names resolve to a private address (Pi-hole with `localIP`, or split DNS) |
| `piholeDnsPublishPorts` | `{ splitDnsActive, localIP }` | Docker port specs for Pi-hole; **internal** |
| `mkUpdaterPaths` | appdata path | `{ stateDir, dockerHistoryDir, systemHistoryDir, dockerLast, systemLast }`; **internal** |
| `hermesProviderEnvVars`, `hermesProviderIds`, `hermesProviderCatalog`, `hermesProviderById`, `mkHermesLlmEnv { provider ? null, apiKey ? null }`, `mkNeoHermesAuth pkgs script` | | Hermes LLM provider catalog and helpers; **internal** |

## Real-world plugins

| Plugin | Patterns worth copying |
|--------|------------------------|
| [highsea.neo](https://github.com/madebydamo/highsea.neo) | Many services in one plugin; every service has `option`/`default`/`swag`/`skills`; an extra flake input (`declarr`) used as a host package through `inputs.declarr.packages.${pkgs.stdenv.hostPlatform.system}`; one service reading other services' options (`config.neo.services.sonarr.apiKey or …`) to wire them together; a native service declared with `mkSystemdUnits`; Jellyfin with `/dev/dri` and the media volume |
| [portrait](https://github.com/madebydamo/portrait) | Rust server and OCI image built by the plugin (`nix2container`, `perSystem.packages`), pulled into NixOS modules with `withSystem`; a oneshot that loads the image before `docker-portrait-server`, listed in `extraUnits`; `mkVpnOptions` for outbound traffic; `auth.enabled = false` for a public site; secrets as `nullOr str` options filtered out of `environment` when unset |
| Neo's own `nix/services/` | `vikunja` (native module in appdata), `paperless` (database sidecars, secrets with assertions), `syncthing` (setup unit, protocol ports), `nextcloud` / `collabora` (setup units), `backup` (timer units) |

## See also

- [Installation](INSTALL.md): first-time setup
- [How it works](ARCHITECTURE.md): security and traffic model
- [CLI](CLI.md): `neo update`, `neo activate`, `neo build`
- [AGENTS.md](../AGENTS.md): development rules for Neo and plugins
- [README](../README.md): product overview
