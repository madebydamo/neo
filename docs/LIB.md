# `lib.neo` reference

Everything a plugin (or a core service) can read from `config.neo` and call from `lib.neo`. How to put a plugin together: [Plugin development](PLUGIN-DEVELOPMENT.md).

## `config.neo` values

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

## `lib.neo` functions

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
| `mkServiceMeta` | `{ icon ? null, tagline ? "", description ? "", notes ? "", replaces ? [], privacy ? [], license ? null, deprecated ? null, projectUrl ? null, githubUrl ? null, releaseUrl ? null, screenshots ? [], rank ? null, iframeCompatible ? true, category ? "Other" }`. `icon` is a URL string or a path (`./icon.svg`, png, webp, ico); paths are served at `/service-icons/…`. `screenshots` is a list of `{ url, caption ? null }`. `tagline` is one line for the grid card (empty: first line of `description`), `notes` the collapsed "Good to know" for operators, `replaces` familiar products (`["Google Photos"]`), `privacy` privacy map category ids. `license` is `{ spdx, url, openSource ? true, note ? "" }` (linked in the pane; `openSource = false` shows "Not fully open source" with the note). `deprecated` is `{ reason, instead, alternatives ? [] }`: the service is hidden from the grid and privacy map unless enabled, and the pane shows the notice with links to the alternatives | Internal `meta`, shown on the Services grid, navigator and option pane. `rank` sorts the grid; set `iframeCompatible = false` for apps that refuse to load in the navigator iframe |
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
- **Required secrets:** `assertions = lib.neo.requireSecrets "myapp" cfg [ "dbPassword" { name = "secretKey"; minLength = 50; hint ? …; } ];`. A placeholder counts as set, and the length is only checked on a plain value (`lib.neo.secrets.isLongEnough n v`, for your own rules). `lib.neo.isSet v` is the plain "neither null nor empty" test.

The build fails if a placeholder would end up in a container or unit `environment` or in `/etc` text. Rendered files live under `lib.neo.secretsDir` (`/run/neo-secrets`); use the `.path` of a template rather than building paths yourself.

### Containers and units

| Function | Arguments | Returns |
|----------|-----------|---------|
| `toOciBindMounts` | List of `{ localPath, containerPath }` | List of `"host:container"` strings |
| `mkPostgresSidecar` | `config`, `{ name, image, appdata, db, user ? db, password ? null, dataPath ? "/var/lib/postgresql/data", dataDir ? "pgdata", environment ? {}, health ? null, extraOptions ? [] }` | Module fragment for `mkMerge`: the Postgres container on `internal`, its data directory, `POSTGRES_PASSWORD` through the secrets |
| `mkRedisSidecar` | `config`, `{ name, image, appdata ? null, dataDir ? "redisdata", cmd ? [], health ? null }` | Module fragment for `mkMerge`: the Redis container on `internal`; without `appdata` it keeps nothing on disk |
| `dockerHealthOptions` | `{ cmd, interval, timeout, retries, startPeriod ? null }` | `--health-*` flags for `extraOptions` |
| `linuxserverEnv` | `config` | `{ PUID; PGID; TZ; }` from `neo.core.uid`, `gid` and `timeZone`; merge into a container's `environment` with `//` |
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
| `mkSubdomainProxyConf` | `{ config, cfg, upstream ? null, port ? null, proto ? "http", proxyPass ? null, maxBodySize ? "0", auth ? true, includeResolver ? (proxyPass == null), geo ? true, serverExtra ? "", locationExtra ? "", extraLocations ? "" }`. Give `upstream` + `port`, or `proxyPass` (full URL). `maxBodySize = null` omits `client_max_body_size`; `"0"` is unlimited | Full nginx `server { … }` block for `<subdomain>.*` with listen, SSL, geo, `proxy.conf`, resolver and tinyauth |
| `mkSwagServer` | `{ cfg, maxBodySize ? "0", geo ? true, extra ? "", body }`. `extra`: more server-level directives; `body`: the locations | The `server { … }` wrapper, for a vhost that needs its own locations |
| `swagUpstream` | `{ app, port, proto ? "http", proxyConf ? true, extra ? "" }`. `extra` goes before `proxy_pass`; `proxyConf = false` is for a location that sets its own headers | Lines of a location body that proxy to a container |
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
| `visibleWhen` | `"enabled"` hides the field while that sibling bool is false (or that sibling string is empty); `"type=entra"` hides it unless the sibling equals the value; `"format=files,encrypted"` unless it equals one of them. Works inside attrsOf entries too |
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
| `hermesProviderEnvVars`, `hermesProviderIds`, `hermesProviderCatalog`, `hermesProviderById`, `mkHermesLlmEnv { provider ? null, apiKey ? null }`, `mkNeoHermesAuth pkgs script`, `hermesCustomKeyEnv` | | Hermes LLM provider catalog and helpers; **internal** |
