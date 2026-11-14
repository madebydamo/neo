# Network isolation: one Docker network per service

Status: implemented. Code: `nix/lib/docker.nix` (`lib.neo.docker`),
`nix/modules/core/docker-network.nix`, check `nix/output/network-isolation-check.nix`.

## Goal

Before, every app container sat on one bridge network, `internal`, and the host
accepted any connection from any Docker bridge. Any container could reach any
other (a compromised Pastebin could talk to the Immich database or Vaultwarden
directly, without SWAG and tinyauth) and the host's own services: the Neo web UI
answered every container without a login.

Now:

- Each built-in service has its own network `app-<service>`
  (`lib.neo.docker.serviceNetwork`). Only that service's containers are on it,
  plus SWAG when the service has a site and gluetun when it is VPN-routed.
- SWAG joins the networks of the services it proxies.
- A service that needs another service's container declares a **link**
  (`lib.neo.docker.mkLink`): a small network with just those containers.
- Host services (neo web, Hermes, Vikunja, security, backup-store,
  iSponsorBlockTV) answer only the `neo-host` bridge, which SWAG and Grafana
  join (`lib.neo.docker.mkHostAccess`).
- **`internal` stays as it was** for plugins: same name, SWAG on it, everything
  on it reaches everything else on it. Plugins opt in by switching to
  `serviceNetwork`; nothing is rewritten behind their back.
- Networks are declared by naming them; `neo-docker-network` creates and
  removes them.
- The flake check `docker-network-isolation` and the VM smoke test prove it.

Kept: one gluetun shared by all VPN-routed apps (see VPN).

## Who talks to whom

| Who | How | Now |
|-----|-----|-----|
| SWAG → every app | Container name (`set $upstream_app <name>`) | SWAG on each proxied service's network |
| SWAG → tinyauth | `http://tinyauth:<port>/api/auth/nginx` | tinyauth is an app with a site |
| SWAG → host services | `host.docker.internal` | `neo-host` (gateway `172.31.255.1`) |
| SWAG `proxyPass` routes | Operator-entered `http://host:port` | SWAG stays on `internal`, so plugin containers keep working; a built-in container needs its public URL |
| Inside one service (app → db/redis/chrome/meilisearch/webengine/sso/mcp/cron) | Container names | Same network |
| SWAG → crowdsec | `http://crowdsec:8080` | Both in the `swag` service (`app-swag`) |
| Nextcloud → Collabora | `wopi_url = http://collabora:9980` | Link `link-nextcloud-collabora` |
| Grafana → Hermes webhook | `host.docker.internal` | Grafana joins `neo-host` while alerts are on |
| Monitoring agent (Alloy, host network) → app metrics | Docker discovery on `network` | Default: the container's first network. Host → bridge is not blocked |
| netbird-server setup | `docker inspect` of its address | Reads `app-netbird-server` |
| VPN-routed containers | `networks = ["container:vpn"]` | gluetun on the routed services' networks, aliases per network |
| Apps → other apps' public URLs (immich-drop → Immich, ical-sync/calino → Rustical, Collabora → Nextcloud callback) | `https://<sub>.<domain>` through the host and SWAG | Unchanged (published ports are forwarded, not host input), and the preferred way between apps |
| Hermes skills | Native, with Docker access: `docker exec swag curl …` | Unchanged (SWAG is on every network) |
| Plugins (high_sea, fortilegends, example) | `networks = ["internal"]`; high_sea's declarr calls radarr, sonarr, prowlarr, … | Unchanged. None calls a built-in container by name |

**The one behaviour change for plugins:** built-in containers left `internal`,
so a plugin container cannot call a built-in one by name (e.g.
`http://immich-server:2283`) or reach a host service through its gateway. None
of the known plugins does; such a plugin uses the public URL, a link or
`mkHostAccess`. `docs/PLUGIN-DEVELOPMENT.md` says so.

## Design

### Network tooling

`neo.core.docker.networks.<name> = { internal; subnet; gateway; bridgeName; }`
is filled from the containers: every network a container names (in `networks`
or `--network=` in `extraOptions`) is declared, plus `internal` and
`swag-ipv6`. `neo.core.dockerNetworks` (list) still works.

`neo-docker-network.service` (oneshot, every container `After`/`Wants` it):

- Creates missing networks with `--label neo.managed=1`; fixed-subnet networks
  first, before the pool could hand out their range.
- Leaves an existing unlabelled network of the same name alone (`internal`).
- Recreates a network whose flags differ once no container uses it; until then
  it logs.
- Removes managed networks no one declares once empty, so a disabled service
  leaves no network behind. The pre-label `monitoring` network goes the same
  way. `internal` is never removed.

Docker's default pools give about 31 bridge networks (172.17–31.0.0/16 plus
16 × /20 of 192.168.0.0/16), too few for 40 services, and the 192.168 slices
collide with home networks. `default-address-pools = [{ base = "172.16.0.0/12";
size = 24; }]` gives 4096 /24 networks, in the range the NetBird check already
reserves for Docker. Existing networks keep their subnets. Changing the daemon
settings restarts Docker once on upgrade.

### Service networks

Built-in services name `lib.neo.docker.serviceNetwork "<service>"`;
`mkPostgresSidecar` and `mkRedisSidecar` take `network` (default `internal`, so
plugin callers are unchanged).

Rejected: rewriting `internal` to the owning service's network automatically.
It breaks high_sea, whose separate services talk to each other. Rejected: one
network per container with allow lists (a bridge has no per-pair rules).

### SWAG

SWAG joins its networks with `docker run --network=…`: `internal`, `swag-ipv6`,
`neo-host`, and `app-<service>` of every enabled proxied service that has
containers there. That list changes exactly when a site is added or removed,
which already changes SWAG's `SUBDOMAINS` / `EXTRA_DOMAINS` and restarts it, so
it adds no restarts. Restarting an app does not touch SWAG.

### Links

```nix
(lib.neo.docker.mkLink { name = "nextcloud-collabora"; containers = ["nextcloud" "collabora"]; })
```

The link network goes after each container's own network (`mkAfter`): tools
that read the first network (monitoring, `docker inspect`) keep the service
network. An assertion fails when a link container does not exist or cannot join
(a VPN-routed container shares gluetun's namespace).

### Host services

The host used to accept every bridge (`iptables -I nixos-fw -i br+ -j ACCEPT`)
and DNAT loopback services from every bridge. Now the network `neo-host` has a
fixed bridge name and subnet (`172.31.255.0/24`); the firewall accepts and
DNATs from `-i neo-host` only, and its members get
`--add-host=host.docker.internal:172.31.255.1`. Members: SWAG, and Grafana
while alerts go to Hermes. The firewall start removes the old `br+`/`docker0`
DNAT rules. Containers keep the internet and published ports (Docker forwards
those; they never hit the host's input chain).

### VPN (one shared gluetun)

- gluetun joins the networks of every VPN-routed service, with the aliases of
  the routed containers per network (`--network=name=app-searxng,alias=searxng`).
  `--network-alias` would apply to the first network only (verified on
  Docker 29), and per network `app-firefox` does not resolve `searxng`.
- `vpn.networks` is applied with `mkDefault`: a container that names its own
  networks keeps them. Adding it on top listed the network twice, which
  Docker 29 refuses (`network … is specified multiple times`); that also broke
  non-routed high_sea services.
- Known limit, by choice: routed containers share gluetun's network namespace,
  so they reach each other on localhost, and gluetun sits on all their
  networks (including `internal` for routed plugin containers).

### Monitoring

`neo.monitoring.apps.<service>.metrics.<e>.network` defaults to the
container's first network. Grafana, Prometheus and Loki moved from their own
`monitoring` network to `app-monitoring`, which only SWAG joins besides them.

## Checks

- **`docker-network-isolation`** (flake check): every built-in service on,
  searxng and changedetection VPN-routed, crowdsec on. Fails when a network
  other than `internal`/`swag-ipv6`/`neo-host` holds containers of two
  services (SWAG and gluetun excepted), a link holds other containers than
  declared, a built-in container is on `internal` or on Docker's default
  bridge, a container names a network twice, SWAG misses a proxied service's
  network, gluetun lacks per-network aliases, `neo-host` has other members
  than SWAG and Grafana, or a container uses `host-gateway`.
- **VM smoke test** (AGENTS.md): SWAG reaches every app; a container does not
  resolve, nor connect by IP to, another service's container; plugins on
  `internal` still resolve each other.

Verified in the VM (Docker 29.9): upgrade from an install with `internal` and
`monitoring` (networks created, `monitoring` removed, all apps up); live switch;
SWAG → every app (HTTP 200), host services (neo web, Hermes) and crowdsec;
Nextcloud ↔ Collabora over the link, Collabora not reaching the Nextcloud
database; no cross-service name resolution, connections by IP time out
(Nextcloud → Immich database, plugin → Immich, Pastebin → Grafana and Loki);
no container but SWAG/Grafana reaches host ports; internet and hairpin to
published 443 still work; high_sea's sonarr → radarr/prowlarr on `internal`;
VPN aliases on `app-searxng` and `internal`; Immich metrics scraped on
`app-immich`; disabling a service removes its network and SWAG leaves it.

## Follow-ups

- `internal = true` (no internet) for databases and caches: a second network
  `app-<service>-data` with the app and its sidecars only.
- Move high_sea and fortilegends to `serviceNetwork` (+ one link for the
  high_sea services that talk to each other) in their own repos.
- Host-network containers (Alloy, beszel-agent, iSponsorBlockTV) are outside
  this scheme.
- Show a service's network and links in the web UI's Advanced section.
