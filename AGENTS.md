# AGENTS.md — Neo development guide

Map for humans and coding agents. Deeper docs: [README.md](README.md), [docs/INSTALL.md](docs/INSTALL.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/CLI.md](docs/CLI.md), [docs/PLUGINS.md](docs/PLUGINS.md).

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
| `just exec 'CMD'` / `just logs SVC` / `just ssh` | Guest shell / `journalctl -b -u SVC` |
| `just format` / `just check` | alejandra + cargo fmt / flake check |

**Flakes only see git-tracked files** — `git add` new `nix/` paths before `just launch`.

## Add a service

1. `option.nix` — `enabled`, `// lib.neo.mkReverseProxyOptions`, `mkContainerDefinitions`, `mkAppdata`, `mkServiceMeta`, `mkSkillOptions`
2. `default.nix` — `mkIf`, activation dirs, `oci-containers` on `networks = ["internal"]`
3. `swag.nix` if public; `skills.nix` for Hermes (`mkServiceSkill`)
4. Enable in **`settings.toml`**: `[services.<name>] enabled = true`
5. `git add` → **`just launch`** → smoke test below → `just format && just check`

Naming: options snake_case under `neo.services.*`; plain-string descriptions; volumes via `config.neo.core.volumes.*`.

**SWAG traps:** `include /config/nginx/proxy.conf` already sets Upgrade/Connection and proxy timeouts — **do not re-set** them (426 WebSockets / `proxy_*_timeout` duplicate kills all vhosts).

**tinyauth:** default edge auth. Health probes need `auth.publicPaths` (e.g. `^/api/v1/info/status$`). UI stays 302 → tinyauth.

**Hermes:** per-service `skills.nix` → `neo-<name>`; use `mkServiceSkill`; credentials = real settings keys only. Collector: `nix/services/hermes/skills.nix`.

**Rust:** clap, `anyhow::Result`, `?` + `.context`, `toml_edit::DocumentMut`; no `unwrap` in lib paths.

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
