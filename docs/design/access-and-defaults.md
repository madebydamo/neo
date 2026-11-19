# Access control and inherited defaults

Two parts that share one mechanism:

1. **Inherited defaults.** A setting can take its default from another setting
   (server name, domain, default access). The web UI shows it the same way
   everywhere.
2. **Access control.** A default access for all apps (where they open from,
   login page, sign-in with the login page account) that every app follows
   until it is changed for that app, and an **Access** page (Settings → Access &
   backup → Access) that shows and edits every way into the server.

## Part 1 — Inherited defaults

### Rule

| The default is … | Declare it with | UI |
|---|---|---|
| another setting, or computed from it | `lib.neo.mkInheritedOption { from = "core.hostname"; default = …; }` | the value, chip **from Server name** (links to the source), **Use Server name** once changed |
| decided later, or "off" when empty | `nullOr` + `ui.emptyMeans = "Nix decides";` | the text as placeholder instead of "Not set" |
| a constant | plain `mkOption` | the value |

- `default` is the real value, so consumers read the option directly. No
  `nullOr` + null check in `default.nix`.
- A key in `settings.toml` overrides. Removing the key follows the source
  again. The form drops a value equal to the default, so "the same as the
  source" means "follow it".
- `from` is a settings path (`core.hostname`,
  `services.netbird-server.networkRange`, `access.defaultIngress`).
  `fromLabel` overrides the source's `ui.label`.
- `fromModule = true`: the option is declared where `config` is not
  available (inside `mkReverseProxyOptions`, `mkSsoOption`). A module gives it
  the source's value with `mkDefault` (`nix/modules/core/access.nix` does this
  for every service option marked so), and the extractor reads the source's
  value as the default.

### Pieces

| Piece | Where |
|---|---|
| Helper | `nix/lib/option.nix` (`mkInheritedOption`), `nix/lib/ui.nix` (`emptyMeans`) |
| Extractor | `extract_service_options.nix` (`inheritsMeta`: `inherits = { path; label; fromModule; }`) |
| Rust | `types/schema.rs` (`OptionInherits`, `OptionUi.empty_means`), `nix/sections.rs` (label, `url`) |
| Templates | `options/field_head` (chip, "changed here", "Use …"), `options/str`, `options/null_or` |
| Check | `inherited-defaults`: evaluates every service with a sentinel server name and domain. A visible default that contains them without `inherits` fails, and so does a description that says what empty falls back to ("Empty uses …") without `emptyMeans` or `inherits` |

Converted: git email, backup folder and store name, Tailscale / NetBird /
monitoring device names (from the server name); Gitea web and SSH addresses;
NetBird management URL and range; the CalDAV address of ical-sync; the Nix
build limits got `emptyMeans`.

## Part 2 — Access control

### Default access (`neo.access`, `nix/modules/core/access.nix`)

| Option | Default | Followed by |
|---|---|---|
| `defaultIngress` | all four places | each app's Reachable from (`mkReverseProxyOptions`) |
| `defaultLogin` | `true` | each app's Require login, if the app lets the user choose and does not fix it |
| `defaultSso` | `false` | each app's `sso` (`lib.neo.mkSsoOption`); needs tinyauth (assertion) |

An app that must not follow passes the value to `mkReverseProxyOptions`
(`ingress = ["local"]`, `auth.enabled = false`). Docmost keeps its own SSO
bridge (on by default), it is not an OIDC client.

`settings.toml`: `[access]` (a top-level table like `disko`, see
`util::TOP_SECTIONS`).

### SSO

`mkSsoOption` declares `sso` (rank 125, Access group). Inside `mkIf cfg.sso`
the app registers `neo.services.tinyauth.oidcClients.<id>` and reads the
client id, secret and issuer (example: `nix/services/vaultwarden`). Users are
then managed on the login page: its own users or an OAuth provider.

Done: Vaultwarden. Candidates (OIDC upstream, each needs a VM login test):
Immich, Mealie, Paperless, Karakeep, Gitea, Nextcloud (user_oidc), Vikunja,
Dawarich, Grafana (Monitoring), RustiCal, Beszel.

### Exposure (`neo.exposure`, `nix/modules/core/exposure.nix`)

- `neo.exposure.ports.<id> = { port; protocol; interface; label; service; login; open; }`
  — declared by the module that opens the port.
- `neo.exposure.report` (internal): apps (hosts, places, login page, SSO,
  `meta.ownLogin`, who may open them from tinyauth's ACLs), ports discovered
  from the firewall (global and per interface) and published Docker ports not
  bound to loopback, each matched to its description, SSH (keys, password
  login, console password, NetBird SSH) and the login page (users, providers,
  groups, OIDC apps).
- `mkServiceMeta { ownLogin = "Immich accounts"; }`: the app's own accounts. Computed
  from the app's settings in `option.nix` when it depends on them (Pi-hole
  password, registry users). Never set `meta` from `default.nix` under
  `mkIf cfg.enabled`: that recurses.
- Check `exposure-complete`: every open port, with every service on, has a
  description.

### Access page

`/configuration/access` (`routes/access.rs`, `templates/access.html.hbs`),
eval query `QueryKey::Access` (`extract_access.nix`).

1. Warnings: app on the internet with no login (error when a login was turned
   off, warning when the app has none by design), apps asking for login while
   tinyauth is off, SSH password login, undescribed ports.
2. Default for all apps: place buttons, login and SSO toggles.
3. Apps: a row per app, a toggle per place, login page and SSO; badges
   "changed" / "set by app"; **Use defaults** removes the app's values.
4. Open ports: port, what it is for, where it is open, what protects it.
5. Logins: SSH keys (type, name, last characters), password login, console
   password, NetBird SSH; login page users, providers, groups, OIDC apps.

Each click posts `path` + JSON `value` to `/access/set`, which builds a patch
for `apply_edit` (settings lock, secrets routing, like the form). Only the
three defaults and an app's `ingress` / `auth.enabled` / `sso` are accepted. A
value equal to what the setting follows becomes an unset. The page reads the
editable values from `settings.toml` and only the structure from the
evaluation, so it shows a click's result before the next evaluation.

## Later

- SSO for the candidate apps above.
- Per-app user access (tinyauth `access` / `groupAccess`) edited from the apps
  table.
- Other global defaults worth the same treatment: outgoing mail (SMTP) for every
  app that sends mail, an admin e-mail address, automatic image updates, a
  default for the AI assistant skills, a language / locale.
- Ingress for non-HTTP ports (SSH, Syncthing) with per-interface firewall
  rules: out of scope for now.
