# Network isolation: one Docker network per service (plan)

Status: plan, not implemented.

## Goal

Today every app container sits on one bridge network, `internal`. Any container
can open a connection to any other: a compromised Pastebin can talk to the
Immich database or Vaultwarden directly, without going through SWAG and tinyauth.

After this change:

- Each built-in service gets its own network `app-<service>`. Only that
  service's containers are on it (plus SWAG when the service has a site, and
  gluetun when it is VPN-routed).
- SWAG joins the networks of the services it proxies.
- A service that really needs another service's container declares a **link**:
  a small network with just the containers involved.
- **`internal` stays exactly as it is** for plugins: same name, SWAG on it,
  every container that names it can reach every other one on it. Plugins opt
  into isolation by switching to `lib.neo.docker.serviceNetwork`; nothing is
  rewritten behind their back.
- `lib.neo` gets declarative Docker network tooling, so nobody writes
  `docker network create` in a script again.
- A flake check and the VM smoke test prove isolation.

Out of scope (follow-ups at the end): no-internet networks for databases,
host-network containers. One gluetun shared by all VPN-routed apps stays.

## Today (inventory)

| Who | How they talk | Effect of isolation |
|-----|---------------|---------------------|
| SWAG → every app | Container name (`set $upstream_app <name>`) | SWAG joins each proxied service's network: unchanged |
| SWAG → tinyauth | `http://tinyauth:<port>/api/auth/nginx` (`nix/lib/reverseProxy/authorization.nix`) | tinyauth is an app with a site: unchanged |
| SWAG → native services (neo web, Hermes, Vikunja, security, backup-store, iSponsorBlockTV) | `host.docker.internal` | Unchanged |
| SWAG `proxyPass` routes | Operator-entered `http://host:port` | SWAG stays on `internal`, so plugin containers keep working; a route to a built-in container by name would need the public URL (documented) |
| Inside one service (app → its db/redis/chrome/meilisearch/webengine/sso/mcp/cron) | Container names | Same network: unchanged |
| SWAG → crowdsec | `http://crowdsec:8080` | Both in the `swag` service: unchanged |
| **Nextcloud → Collabora** | `wopi_url = http://collabora:9980` (`nix/services/collabora/default.nix`) | **Needs a link** |
| Grafana / Prometheus / Loki + SWAG | Own `monitoring` network already | Becomes `app-monitoring` through the generic mechanism |
| Monitoring agent (Alloy, host network) → app metrics | Docker discovery filtered on `network` (default `"internal"`, `nix/modules/core/monitoring.nix`) | Default follows the container's network. Host → bridge is not blocked by Docker |
| netbird-server setup | `docker inspect … Networks "internal"` (`nix/services/netbird-server/default.nix`) | Read its own network |
| VPN-routed containers | `networks = ["container:vpn"]`; gluetun joins `vpn.networks` of every routed service with `--network-alias=<routed container>` | See VPN below |
| Apps → other apps' public URLs (immich-drop → Immich, ical-sync/calino → Rustical, Collabora → Nextcloud callback) | `https://<sub>.<domain>` through the host and SWAG | Unchanged, and the preferred way between apps |
| Hermes skills ("curl http://bentopdf:8080 from internal network") | Hermes is native with Docker access, uses `docker exec swag curl …` | Works; wording of a few skills changes |
| Karakeep → `http://ollama:11434` | Only an example in an option text | Text: use a public URL |
| beszel-agent (host network) → `http://beszel:8090` | Container name from the host network | Check whether that works today at all; no isolation change |
| **Plugins** (high_sea, fortilegends, example) | `networks = ["internal"]`; high_sea's declarr calls `radarr`, `sonarr`, `prowlarr`, `lidarr`, `bazarr`, `tdarr`, `deemix`, `flaresolverr`, all in high_sea; qbittorrent via gluetun | Stay on `internal`: unchanged. None of them calls a built-in container by name |

Searched: every `http://`/`redis://`/`postgres://` host in `nix/`,
`templates/` and the local plugin checkouts, every `networks =`, `--network`,
`--network-alias`, `--add-host`. No other cross-service container name is used.

**The one behaviour change for plugins:** built-in containers leave `internal`,
so a plugin container that called a built-in one by name (e.g.
`http://immich-server:2283`) would no longer resolve it. None of the known
plugins does; such a plugin uses the public URL or a link (`mkLink` works across
plugin and core). Called out in the release notes and `docs/PLUGIN-DEVELOPMENT.md`.

## Design

### 1. Network tooling: `neo.core.docker.networks`

Replaces `neo.core.dockerNetworks` (list, kept as an alias for one release) and
the hand-written script in `nix/modules/core/docker-network.nix`.

```nix
neo.core.docker.networks.<name> = {
  ipv6 = false;      # IPv6-only network (swag-ipv6 today)
  internal = false;  # docker --internal: no route to the internet (follow-up use)
};
```

`internal` (the network) and `swag-ipv6` are declared through it too, always.

`neo-docker-network.service` reconciles instead of only creating:

- Creates missing networks with `--label neo.managed=1` plus the flags.
- Adopts an existing network of the same name without the label (the
  `internal` and `monitoring` networks of current installs), never fails on it.
- Recreates a network whose flags changed only when no container uses it;
  otherwise logs and keeps it.
- Removes managed networks that are no longer declared and have no containers,
  so a disabled service leaves no network behind. `internal` is never removed.

**Address space.** Docker's default pools give about 31 bridge networks
(172.17–31.0.0/16 plus 16 × /20 of 192.168.0.0/16). 40 services plus links run
out, and the 192.168 slices collide with home LANs. Set
`virtualisation.docker.daemon.settings.default-address-pools =
[{ base = "172.16.0.0/12"; size = 24; }]`: 4096 /24 networks, inside the range
the NetBird assertion already reserves for Docker. Existing networks keep their
subnets; new ones get /24s.

Lib helpers (`nix/lib/docker.nix`, documented in `docs/LIB.md`):

| Helper | Returns |
|--------|---------|
| `lib.neo.docker.serviceNetwork name` | `"app-${name}"` (plugins pass `plugin.name "<service>"`) |
| `lib.neo.docker.mkLink { name; containers; }` | Module fragment: network `link-<name>`, joined by exactly those containers |
| `lib.neo.docker.containerService config` | `{ <container> = <service>; }` from `containers` keys and `docker-*` entries of `systemdUnits`; used by the check and monitoring |

### 2. Service networks

A module `nix/modules/core/service-networks.nix` declares `app-<service>` for
every enabled service that has a container on it (detected from the containers'
`networks`, so a plugin that opts in gets its network created the same way).

Built-in services switch explicitly: the ~40 `networks = ["internal"]` lines,
`mkPostgresSidecar`, `mkRedisSidecar` and the `mkVpnOptions { networks = … }`
calls take `lib.neo.docker.serviceNetwork "<service>"`. The sidecar helpers get
a `network` argument (default `"internal"`, so plugins calling them are
unchanged); built-in callers pass the service network.

`"internal"` is never rewritten. Rejected: rewriting `internal` to the owning
service's network automatically (breaks high_sea, whose services talk to each
other). Rejected: one network per container with allow lists (a bridge has no
per-pair rules; it would need our own iptables).

The plugin template moves to `serviceNetwork` so new plugins start isolated; a
plugin whose services talk to each other uses one `mkLink` for that group, or
stays on `internal`.

### 3. SWAG

SWAG joins its networks through `docker run --network=…` flags as today:
`internal`, `swag-ipv6`, and `app-<service>` of every enabled service that has
a site (a `proxyConf`). Services without a site are not joined.

That list changes exactly when an app with a site is enabled or disabled, which
already changes SWAG's `SUBDOMAINS` / `EXTRA_DOMAINS` environment and restarts
SWAG. So no extra restarts, and no runtime `docker network connect` machinery.
Restarting an app does not touch SWAG.

The monitoring module's `swag.extraOptions = ["--network=monitoring"]` goes
away: covered by the rule above.

### 4. Links between services

```nix
# nix/services/collabora/default.nix (inside mkIf nextcloud.enabled)
(lib.neo.docker.mkLink { name = "nextcloud-collabora"; containers = ["nextcloud" "collabora"]; })
```

Only `nextcloud` and `collabora` sit on `link-nextcloud-collabora`; the
Nextcloud database does not see Collabora. A link is an explicit, reviewable
statement in the service that needs it. Prefer the public URL when the app
supports it; a link is for internal-only endpoints (WOPI here).

`mkLink` appends the network to each container's `networks`, so it works on
any container, core or plugin.

### 5. VPN (one shared gluetun, as today)

gluetun joins the networks of every VPN-routed service (`vpn.networks`), and
routed containers share its network namespace:

- SWAG and the service's own containers reach routed containers through
  gluetun's aliases. With several networks the alias must be set per network
  (`--network=name=app-searxng,alias=searxng`): a single `--network-alias`
  applies only to the first network. Verify on Docker 29 in the VM.
- Each alias goes on the routed service's own networks only, so `app-firefox`
  does not resolve `searxng`.
- Containers routed through gluetun share one network namespace, so they reach
  each other on localhost, and gluetun sits on all their networks. That's
  accepted by choice and documented as a known limit.

### 6. Monitoring

- `neo.monitoring.apps.<service>.metrics.<e>.network` defaults to the first
  bridge network of `container` (from its `networks`), so built-in apps get
  `app-<service>` and plugins on `internal` keep `internal`.
- The `monitoring` service's own network becomes `app-monitoring`; the old
  `monitoring` network is removed once empty (it is adopted as managed).

## Checks

- **Flake check `docker-network-isolation`** (eval of the homeserver template
  with many services on): every network except `internal` and `swag-ipv6`
  holds containers of one service, or is a `link-*`; the only allowed foreign
  members are SWAG and gluetun. No built-in container is on `internal`. Fails
  with the offending network and containers.
- **Eval assertion**: `mkLink` containers exist.
- **Plugin check**: build the dev config with high_sea enabled; its containers
  are still on `internal` and declarr's targets resolve (VM).
- **VM smoke test** (added to AGENTS.md):

  ```bash
  just exec 'docker exec swag curl -sS -m 5 -o /dev/null -w "%{http_code}\n" http://immich-server:2283/'   # reachable
  just exec 'docker exec nextcloud curl -sS -m 5 http://collabora:9980/hosting/discovery | head -c 80'       # link works
  just exec 'docker exec pastebin getent hosts immich-database || echo isolated'                            # must print isolated
  just exec 'docker exec declarr getent hosts radarr'                                                       # plugin on internal unchanged
  just exec 'docker network ls --filter label=neo.managed=1'
  ```

  Plus the usual per-service checks for every service enabled in the dev
  settings, since every built-in container is recreated once.

## Rollout

1. `nix/lib/docker.nix`, `neo.core.docker.networks`, reconcile script, address
   pools; `neo.core.dockerNetworks` as an alias. (`nix:`)
2. `service-networks.nix`; sidecar helpers get `network`. (`nix:`)
3. Built-in services switch to `serviceNetwork` (one mechanical commit). (`nix:`)
4. SWAG joins proxied services' networks; drop the monitoring `--network=`
   override. (`swag:`)
5. Nextcloud ↔ Collabora link. (`collabora:`)
6. VPN per-network aliases. (`vpn:`)
7. Monitoring default network, netbird-server inspect. (`monitoring:`,
   `netbird-server:`)
8. Flake check + smoke test steps. (`nix:`)
9. Docs: AGENTS.md (Add a service, SWAG, smoke test), `docs/LIB.md`,
   `docs/PLUGIN-DEVELOPMENT.md` (internal stays; how to opt in; built-ins left
   internal), `docs/ARCHITECTURE.md`, plugin template and its AGENTS.md,
   `agent/skills/neo-plugin` and `neo-core`, skill texts that say "from
   internal network", Karakeep's ollama example. (`docs:`, `agent:`)

Upgrade of an existing install: the first switch recreates every built-in
container (their `--network` changes) and SWAG, so apps restart once, like an
image update. Plugin containers are not touched. No data moves.

## Follow-ups

- `internal = true` (no internet) for networks that only hold databases and
  caches: a second per-service network `app-<service>-data` with the app and
  its sidecars, the sidecars off the main one.
- Move high_sea and fortilegends to `serviceNetwork` (+ one link for the
  high_sea group) in their own repos.
- Host-network containers (Alloy, beszel-agent, iSponsorBlockTV) are outside
  this scheme; review whether any can move to a bridge.
- Show a service's network and links in the web UI's Advanced section.
