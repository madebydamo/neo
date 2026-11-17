## When to Use
Platform questions, disk layout, plugins, reverse proxy model, how to apply config, rollbacks.

## Mental model
1. Operator edits **settings.toml** (Neo web UI or editor).
2. NixOS modules under `neo.services.*` turn options into containers, units, SWAG conf, and volumes.
3. **`neo activate`** (or web UI Apply) builds and switches the system generation.
4. App data lives under Neo volumes — never only in the Nix store.

## Volumes (this machine)
| Volume | Path |
|--------|------|
| root | `@root@` |
| data | `@data@` |
| appdata | `@appdata@` |
| media | `@media@` |
| documents | `@documents@` |

Service appdata is typically `@appdata@/<service>`.

## Edge traffic
- **SWAG** terminates TLS on 80/443 (or streamproxy tunnel ports).
- **tinyauth** is the first login gate for most UIs (`tinyauth-location.conf`).
- Each service's containers sit on their own Docker network **`app-<service>`**; other services' containers cannot reach them. SWAG is on all of them (probe with `docker exec swag curl http://<container>:<port>/`). Plugins may still use the shared network `internal`. Host units are reached via `host.docker.internal` from SWAG only.
- Domain: @domain@

## How to change configuration safely
0. If the `neo` MCP tools are available (`neo_settings_*`), use them:
   read with `neo_settings_schema` / `neo_settings_get`, change only with
   `neo_settings_propose`, then ask the user to review the card (link in
   the reply) and wait for `neo_settings_status` = saved / applied. Never
   send secret values: use op `copy` / `move` when another setting has the
   value (a rename is a move, you never see the value), `generate` or `ask`.
   Do not edit settings.toml, decrypt or edit the secrets files, or read
   secrets out of containers; if the tools cannot do it, say so.
1. Prefer Neo web UI (`neo` service / `https://neo.<domain>`).
2. Or edit settings and run `neo activate` (server profile uses `/etc/neo/settings.toml` when present).
3. Use `neo --dry-run activate` when unsure.
4. Never edit `/nix/store`. Do not hand-edit SWAG proxy-confs for durable changes — they are regenerated.
5. Destructive ops (clear appdata, nuke config, disk wipe) require **explicit user confirmation**.

## CLI toolbox
- `neo --help`, `neo activate`, `neo update`, `neo web`
- `systemctl status|restart <unit>`
- `journalctl -u <unit> -b --no-pager`
- `docker ps`, `docker logs <name>`
- Hermes managed skills: `/neo-<service>` (see workspace AGENTS.md index)

## Plugins
Extra flakes listed under Neo plugins (`core.plugins.<id>` with `url` and an optional `prefix`) add services to the same `neo.services.*` option space. Treat them like built-in services once enabled. A plugin added twice has its second copy's services prefixed (`dev-portrait-server`).
Private plugins: `git+ssh://git@<host>/<owner>/<repo>` (machine git SSH key) or `github:<owner>/<repo>` (needs a `core.git.tokens` entry for github.com).

## Git identity
You commit, clone and push as the machine git identity (`core.git`): @gitUserName@ <@gitUserEmail@>.
- SSH (key `@gitSshKey@`, public key in the Neo web UI): @gitSshHosts@
- HTTPS tokens: @gitTokens@
- Do not create your own SSH keys or git credentials; ask the operator to add a host to `core.git.knownHosts` or a token to `core.git.tokens`.
- Never commit to or push the Neo config repo by hand: configuration changes go through settings and `neo activate`.

## Rollbacks
NixOS generations: bad activate → boot previous generation or `nixos-rebuild switch --rollback` (with care). Data in volumes is independent of generations.

## Verification
- Failed units: `systemctl --failed`
- Containers: `docker ps`
- HTTPS to service subdomains after tinyauth
