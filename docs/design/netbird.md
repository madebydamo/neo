# NetBird: client and self-hosted control plane

Built on 2026-11-11 (commits 08920a4, 546d9fc, de1af06). Two new services: **`netbird`** joins this server to a NetBird
network, the way `tailscale` joins a tailnet. **`netbird-server`** runs the NetBird control plane
and its dashboard, so the network needs no account at netbird.io.

**The goal:** one machine hosts the network, every Neo server joins it, and after that the apps can
be closed to the Internet. §6 walks through exactly that and lists what is missing for it. NetBird's
own ways of publishing services are not built into Neo, but nothing here stands in their way (§7).

Facts about NetBird are from the v0.82.1 sources and docs.netbird.io, read on 2026-11-10.

**Phase 0 (spikes in the VM) is done, 2026-11-10.** Server 0.82.1 and dashboard 2.96.1 ran behind
SWAG in the dev VM, with the nixpkgs client 0.71.4 on the guest, two 0.82.1 clients in containers
and Tailscale (against a local Headscale) next to it. §10 lists what was tested and what came out.
Three things changed the plan:

- Tailscale and NetBird run on one server, so "Reachable from" gets a fourth value `netbird` (§4.1).
- App names need no dnsmasq for NetBird: the control plane has DNS zones (§4.3). The neutral
  `split-dns` module is dropped, Tailscale's stays as it is.
- Only one of the two may manage the host's DNS (§4.2).

**Built (2026-11-11).** What differs from the plan below, found while building and testing:

- **IPv6 at SWAG** (08920a4): SWAG joins a second, IPv6-only Docker network `swag-ipv6`; the apps
  stay IPv4 on `internal`. Giving `internal` IPv6 was tried first and dropped: every app gets an
  IPv6 address in Docker's DNS, its clients try it first, and most apps listen on IPv4 only. A home
  device on a public IPv6 address now counts as Internet (decided).
- **Tailscale state stays where it is.** Tailscale 1.104 seals its state with the TPM by default,
  so a restore onto new hardware would sign in again anyway. Decided: keep the sealing, no move
  (§4.5 is NetBird only).
- **NetBird's resolver on `127.0.0.153`**, not on the NetBird address: containers that copied it
  keep working when the network is re-addressed, and it never collides with Tailscale's dnsmasq.
  Pi-hole needs its home network address with NetBird DNS, as with Tailscale split DNS.
- **No hosts entry** for the client on the hosting machine (§5.4): NetBird's resolver also serves
  `/etc/hosts`, so containers got `127.0.0.1` for the control plane's name.
- **The bootstrap runs `netbird up` itself** with its one-off key, instead of handing the key to
  the client's login unit (which would have to wait for a non-blocking unit).
- **SWAG maps**: the class map has four inputs now for every config, so a config without NetBird
  does not give byte-identical maps; it gives the same classes, which `ingress-posture` checks by
  running nginx on the generated maps.

## 1. NetBird today, as far as it matters here

| Fact | Consequence for Neo |
|------|---------------------|
| Self-hosting is two images since v0.65: `netbirdio/netbird-server` (management, signal, relay, STUN and a built-in login in one process) and `netbirdio/dashboard` | Two containers. The older layout (four services, coturn, an external OIDC provider is required) is what the NixOS module in nixpkgs 26.05 builds: not used |
| One hostname on TCP 443 carries everything: REST, gRPC (HTTP/2), WebSocket (relay, signal). **UDP 3478 (STUN) must be reachable directly** | Fits behind SWAG with a hand-written vhost. STUN cannot go through rathole/streamproxy (§5.5) |
| nixpkgs 26.05 (pinned) has the client 0.71.4 and its module `services.netbird`. No package for the combined server or the reverse proxy | Client native, server as containers |
| Each network gets a random /16 out of `100.64.0.0/10`, the range Tailscale uses too. **The range is a setting of the network** (dashboard, or `network_range` over the API), also outside that block. Changing it gives every device a new address at once, while connected (tested). Interface `wt0`, UDP 51820 | A client cannot choose it; whoever runs the network does (§4.2) |
| Devices also get an IPv6 address (a random `fd…/64` per network) unless the network turns that off (`ipv6_enabled_groups`) | Over IPv6 SWAG does not see the device's address (§4.1) |
| **DNS zones** live in the control plane (`/api/dns/zones`, dashboard: DNS → Zones): A, AAAA and CNAME records, wildcards, handed to every device of a group. Each client answers them from its own resolver | App names without a DNS server on the Neo side (§4.3) |
| On `wt0` the client accepts everything in the host firewall and filters by the network's access rules instead (default: all to all) | Ports closed in the NixOS firewall are open to the network, as with Tailscale (§6) |
| Client settings are split: the machine sets management URL, setup key, hostname, DNS on/off, SSH, routing off. **Groups, access rules, routes, exit nodes, DNS servers and zones are set in the dashboard only** | `advertiseRoutes`, `advertiseExitNode` and `exitNode` of the Tailscale service have no counterpart in settings |
| First user: whoever opens `/setup` first becomes owner. `POST /api/setup` does the same and can return an API token | Neo creates the owner itself and closes `/setup` (§5.3) |
| API tokens live 365 days at most. Setup keys: `expires_in` 1 day to 1 year, or `0` for never (tested). A registered machine stays registered after its setup key expires | No long-lived token on the server; the bootstrap token is used once |
| Reverse proxy (`netbirdio/reverse-proxy`, **beta**, since v0.65) publishes services of the network on the internet. `netbird expose <port>` is the ad-hoc form | The overlap with streamproxy (§2, §7) |
| Licences: client BSD-3-Clause; server, dashboard and proxy AGPL-3.0 | `mkServiceMeta.license` per service |

## 2. Overlaps: what NetBird could do instead of Neo

| # | Neo today | NetBird's way | Decision |
|---|-----------|---------------|----------|
| 1 | `tailscale`: this server in a private network | Same role, open-source control plane | Both services exist and can run at once (§4.2) |
| 2 | Ingress class `tailscale` ("Reachable from") | NetBird devices arrive from the same address range, so today they count as Tailscale without anyone having decided that | Own class `netbird` (§4.1) |
| 3 | Tailscale split DNS: dnsmasq on the tailnet address answers the app names | A DNS zone in the control plane: `*.dev.example.com` → the server's NetBird address | NetBird: one zone per server, no dnsmasq. Tailscale's dnsmasq stays unchanged (§4.3) |
| 4 | `rathole` + `streamproxy`: a VPS forwards 80/443 into a home behind NAT | NetBird as the tunnel between VPS and home; or NetBird's reverse proxy | Not built. Kept possible (§7) |
| 5 | Sharing something quickly with a link | `netbird expose` | Works with the client as soon as the network has a proxy. Skill text with the warning from §7 |
| 6 | `rathole.certificateOnly`: the VPS forward only serves certificate renewal, names resolve privately | Needs private names: NetBird's zones count like Tailscale's split DNS | `privateHostnameDnsActive` learns about NetBird. Port 80 from the Internet is still needed for certificates (§6) |
| 7 | Pi-hole for devices away from home | A nameserver group in the dashboard pointing at the Pi-hole | Works in the spike with a stand-in DNS server, but needs NetBird's own resolver moved off the NetBird address. Not part of phases 1–2 (§4.3, Later) |
| 8 | Tailscale SSH | NetBird SSH | Option `ssh` on the client, off by default |
| 9 | Tailscale exit node / subnet route options | Dashboard settings; the machine only has to forward | One switch "Let this server route for the network"; the rest is a guide step |
| 10 | `backup-store`: SSH inside HTTPS, because the store has no VPN | With both ends in one NetBird network, plain `target = ssh` to the NetBird name works today | Nothing to build. A tip in the backup guide |
| 11 | `vpn` option (gluetun): a container's outgoing traffic through a commercial VPN | Exit nodes route whole machines, not single containers | Different job, no change |
| 12 | tinyauth: who may open an app | Access rules: which device reaches which device | Different layer, both stay. tinyauth also guards requests that come over NetBird |
| 13 | Authentik as login provider | NetBird's built-in login can add an external OIDC provider | Later: a guide, then maybe an option |

## 3. Pieces

| Piece | Where |
|-------|-------|
| Client service | `nix/services/netbird/{option,default,skills,check}.nix` |
| Control plane service | `nix/services/netbird-server/{option,default,swag,skills}.nix`, `bootstrap.sh`, `icon.svg` |
| Ingress class | `nix/lib/reverseProxy/{ingress,reverseProxy}.nix`, `ingress-check.nix` |
| App names over NetBird | The zone is created by `netbird-server`'s bootstrap (§5.4) or by hand in the dashboard. `nix/lib/dns.nix`: `privateHostnameDnsActive` also true for NetBird's `appNames`. Tailscale's dnsmasq is not touched |
| Container start waits for host DNS | `nix/modules/core/container.nix` (`neo-host-dns`) |
| Assertions that name Tailscale | `nix/services/pihole/default.nix`, `nix/services/rathole/{default,option}.nix` |
| Exposure labels in the security report | `cli/src/commands/security/config.rs` |
| SWAG start check | `nix/services/swag/check.nix`: also catch a literal host in `grpc_pass` |
| Tailscale state into appdata | Not built (§4.5) |
| Guides | new `private-traffic-netbird`, `own-private-network`; edit in `reach-from-internet` |
| Docs | `AGENTS.md` (ingress), `docs/ARCHITECTURE.md`, `INSTALL.md`, `LIB.md`, `PHILOSOPHY.md`, `PLUGIN-DEVELOPMENT.md`, `agent/skills/*/SKILL.md`, `templates/plugin/AGENTS.md` where they list the ingress values |

Found while reading and fixed on its own: SWAG's PROXY-protocol port was published on every
interface. It is on loopback now (commit 4a2476f, check `swag-proxy-protocol-loopback`).

## 4. Client: `neo.services.netbird`

Built on `services.netbird` from nixpkgs (default client: unit `netbird.service`, interface `wt0`,
CLI `netbird`). Category Network.

| Tailscale option | NetBird option | Notes |
|------------------|----------------|-------|
| `enabled` | `enabled` | "Reach this server from your own devices over a private NetBird network" |
| `authKey` | `setupKey` (secret, visible) | Rendered with `neo.core.secrets.templates`, passed as `login.setupKeyFile`. Empty when Neo enrols the server itself (§5.4). Only needed until the server has joined |
| `loginServer` (advanced) | `managementUrl` | Empty: the `netbird-server` on this machine when it is enabled, else netbird.io. Shown, not in Advanced: for every server that joins someone else's network it is the main setting |
| `hostname` | `hostname` | Name in the dashboard |
| `acceptDns` | `acceptDns`, label "Use NetBird DNS", default on | Off = `DisableDNS`. Not together with Tailscale's (§4.2) |
| `splitDns` | `appNames`, label "App names over NetBird" | Says that the network answers this server's app names (§4.3). No dnsmasq behind it |
| `ssh` | `ssh` | |
| `acceptRoutes` | `acceptRoutes`, default on | Off = `DisableClientRoutes` |
| `advertiseRoutes`, `advertiseExitNode` | `routing` (bool, "Let this server route for the network") | `services.netbird.useRoutingFeatures = "both"`. Which routes: dashboard |
| `exitNode`, `exitNodeAllowLanAccess` | none | Dashboard. Said in the service notes |
| — | `networkRange` (label "Network address range") | What the network uses, so SWAG can tell NetBird devices apart. Default `10.213.0.0/16`, the same default `netbird-server` sets (§5.2), so a network hosted by Neo needs no value here. For someone else's network: the range from its dashboard |
| — | `port` (advanced, 51820) | WireGuard port, opened in the firewall |

Rules of the implementation:

- Settings that must follow `settings.toml` go through the module's `config` (merged into the
  client's `config.json` at every start), not through flags of `netbird up`: the module's login
  unit runs `up` only while the client is not registered, so flags would never change again.
- State in appdata (§4.5).
- `systemdUnits`: `netbird`, the login unit, and `netbird-check`.
- `netbird-check` (`mkSetupService`, after the client is connected) compares what was configured
  with what the network does, and fails with the fix in its message: the address on `wt0` lies
  outside `networkRange`; with `appNames` on, a name under the server's domain does not resolve to
  this server's NetBird address. Both mistakes are silent otherwise (requests land in the wrong
  "Reachable from" class, names go to the public address).
- `license = { spdx = "BSD-3-Clause"; … }` without `openSource = false`: unlike Tailscale the
  control plane is open too. The service notes say that the default control plane at netbird.io is
  run by NetBird and that `netbird-server` replaces it with your own.
- `skills.nix`: `netbird status`, how to read "connected but relayed", `netbird expose` with the
  warning from §7, and what lives in the dashboard.

### 4.1 Ingress class

Decided by spike 0.5: **a fourth value `netbird`.** No migration. `defaultIngress` becomes `local,
tailscale, netbird, web`; a service whose list was narrowed by hand stays closed to NetBird until
someone ticks it, which is the safe direction.

SWAG classifies a request by source address. Seen in the VM: a NetBird device arrives with its
NetBird address (`10.213.x.x`), a Tailscale device with its tailnet address, at the same time.
`mkIngressMapsConf` gets a `geo $ingress_is_netbird` with `networkRange` (only when the client is
enabled), and the class map asks it first:

| Source | Class |
|--------|-------|
| PROXY-protocol port | web (unchanged, wins over everything) |
| inside `networkRange` | netbird |
| `100.64.0.0/10`, not in `networkRange` | tailscale |
| private ranges, not in `networkRange` | local |
| anything else | web |

Why NetBird comes first: the default range `10.213.0.0/16` lies in what SWAG otherwise calls "Home
network", and NetBird's own default lies in what it calls Tailscale. An assertion refuses a range
that overlaps the Docker networks (`172.16.0.0/12`).

Touches `choiceLabels`, the option description, `ingress-check.nix` (the table as fixtures; a config
without NetBird must give byte-identical maps), the label match in `security/config.rs`, and the
texts in §3.

**IPv6 is the gap.** A NetBird device that connects to the server's IPv6 NetBird address reaches
SWAG as `172.18.0.1` (Docker's proxy; the `internal` network has no IPv6) and counts as "Home
network". This is not specific to NetBird: every IPv6 connection to SWAG's port loses its source
address this way today, also one from the Internet where the server has a public IPv6 address. For
NetBird the plan avoids it: zone records are A records (§4.3), and a network hosted by Neo is set
up without IPv6 addresses (§5.4). The general case should be fixed on its own (see "Found on the
way" in §10).

### 4.2 Tailscale and NetBird on one server

Tested with both running: each on its own interface and address, both reach SWAG and the host, the
routing tables and marks do not collide. Two rules come out of it.

**1. NetBird's range must lie outside `100.64.0.0/10` while Tailscale runs.** Tailscale's firewall
chain drops packets with such a source that did not come in on its own interface. Tested by moving
NetBird into `100.64.77.0/24`: after a restart of `tailscaled` its chain stood first, and NetBird
devices could no longer ping the server or reach its SSH port. SWAG stayed reachable (Docker
forwards that, the chain does not see it), so the failure is partial and depends on which daemon
started last. Assertion: both enabled → `networkRange` outside `100.64.0.0/10`.

- The range belongs to the network, not to the machine: it is set once (by the bootstrap of
  `netbird-server`, §5.4, or in the dashboard) and applies to every server and phone in it.
- **Set it before the first device joins.** A later change works live, but every device gets a new
  address: zone records point at the old ones, and containers that had NetBird's resolver as
  nameserver lose DNS until they are restarted (both seen in the spike).
- The range must be free on every site: not a home LAN of any server, not `172.16.0.0/12`.

**2. Only one of them manages the host's DNS.** Both register with resolvconf as the only resolver,
and the one that registered last wins:

| Order | Result on the host |
|-------|--------------------|
| Tailscale last | NetBird's zones and device names are gone |
| NetBird last | Both work: NetBird forwards to Tailscale's resolver. But NetBird keeps the upstream it saw at its start: turning Tailscale's DNS off afterwards broke every lookup until NetBird restarted |
| Tailscale's control plane sends a search domain but no nameserver (Headscale without MagicDNS) | `/etc/resolv.conf` without any nameserver |

Assertion: "Use Tailscale DNS" and "Use NetBird DNS" are not both on. The message says which one to
turn off. Devices are not affected: a phone asks its own NetBird or Tailscale client.

The main use of running both is moving from Tailscale to NetBird without losing the way in.

### 4.3 App names (split DNS)

What was planned (a dnsmasq on `wt0`, one nameserver entry per server) is dropped. NetBird's client
runs its own resolver on `<NetBird address>:53`, so a dnsmasq there collides, and the control plane
can answer the names itself.

**One DNS zone per server**, in the control plane:

| Zone | Records |
|------|---------|
| `dev.example.com` | `*.dev.example.com` A → NetBird address of dev; `dev.example.com` A → the same |
| `prod.example.com` | `*.prod.example.com` and `prod.example.com` → NetBird address of prod |

Tested with clients 0.71.4 and 0.82.1:

- `photos.dev.<domain>` and `photos.prod.<domain>` resolve to the two servers, on every device of
  the network, with no setting on the device.
- Names outside the zones still resolve publicly, also other names of the same registered domain.
- A wildcard covers one label. `a.b.dev.<domain>` falls through to public DNS. Neo's app names are
  one label under the server's domain, and custom domains need their own zone.
- A zone for the hosting server's whole domain does not capture the control plane's own name: the
  client keeps resolving its management address publicly. More specific zones win over it.
- Records can be CNAMEs to the device's NetBird name (`neo-dev.netbird.selfhosted`), which follow a
  new address by themselves. Not used: they also hand out the IPv6 address (§4.1) in a network
  that has IPv6 on, which Neo cannot rule out for a network it does not host.
- A device opened an app by name through NetBird and got the app.

| Server | Who creates the zone | When |
|--------|----------------------|------|
| The one that hosts the network | The bootstrap (§5.4) | First start |
| Every other server | You, in the dashboard (DNS → Zones): the server's domain, two records, address from `netbird status` | Once per server. A guide step; the skill reads out the three values, and `netbird-check` says when it is missing |

- With the state in appdata (§4.5) the address survives a reinstall, so the records stay valid.
- New apps need nothing: the wildcard covers them.
- `appNames` on the client is the statement "the zone exists". It makes `privateHostnameDnsActive`
  true, which Pi-hole's and `rathole.certificateOnly`'s assertions ask for, with texts that name
  both networks. Neo cannot see the zone at build time; `netbird-check` verifies it at run time.
- Two servers with the *same* Neo domain work too, with one record per app name instead of the
  wildcard. Not built (see Not planned).
- Tailscale's `splitDns` (dnsmasq on the tailnet address) stays exactly as it is. With both
  networks on, each has its own mechanism and they do not meet.

**Pi-hole for the network** (later, not needed for §6): a DNS server on the NetBird address works
once NetBird's resolver is moved to a loopback address (`dns-resolver.address` of the module).
Devices used it as primary nameserver, zones and device names kept working. Left alone, NetBird
falls back to `127.0.0.1:53` by itself when port 53 on its address is taken, but only if the other
server started first.

### 4.4 Containers and host DNS

With "Use NetBird DNS" on, `/etc/resolv.conf` holds NetBird's resolver only (resolvconf key `wt0`),
and the servers from before become its upstream. Docker reads the file when a container starts:

| Container started | Its lookups go to | NetBird names |
|-------------------|-------------------|---------------|
| before NetBird registered | the normal resolver | no: another server's app resolves to its public address |
| after | NetBird's resolver. Works, also on a loopback address | yes |

- After a reboot of the VM every container was up 20 s before NetBird registered, so all of them
  had the normal resolver. Without a rule the outcome depends on timing.
- A container of the second kind depends on NetBird: with the client stopped, or after the network
  was re-addressed, it resolves nothing until it is restarted. Tailscale has the same property
  today.

`neo-host-dns` already waits for Tailscale's key (90 s at most, then goes on with what is there).
The wait becomes a list of resolvconf keys, and NetBird adds `wt0` when "Use NetBird DNS" is on.
The fixtures in `container.nix` get a NetBird case.

**Not on the machine that hosts the network**: its client can only connect once SWAG and
`netbird-server` are up, so waiting for it would hold every container for the full 90 s. There the
wait is skipped, and containers resolve other servers' apps publicly. Host services (backup over
SSH, `neo` itself) use the host resolver and get NetBird names on every machine.

### 4.5 State in appdata (NetBird; Tailscale dropped)

Built for NetBird. The Tailscale half below was not built: Tailscale seals its state with the TPM
by default, and keeping that was decided over a portable identity.

The state is the machine's identity in the network: a private key and the registration. In appdata
it is covered by snapshots, backup and `neo restore`, like everything else.

| | NetBird | Tailscale |
|---|---------|-----------|
| Today | new | `/var/lib/tailscale` (unit: `--state=/var/lib/tailscale/tailscaled.state`, `StateDirectory=tailscale`) |
| Target | `${appdata}/netbird` | `${appdata}/tailscale` |
| How | Module option `dir.state`; drop `StateDirectory`; tmpfiles rule | `services.tailscale.extraDaemonFlags = [ "--statedir=…" "--state=…/tailscaled.state" ]`. The flags come after the unit's own `--state`, and the last one wins, so `ExecStart` stays untouched. tmpfiles rule. The empty `/var/lib/tailscale` that systemd still creates is harmless |
| Owner | `root`, mode 0700 | `root`, mode 0700 |
| Declared with | `mkAppdata` | `mkAppdata` |

The daemons run as root and the directory holds a private key, so it is the one appdata folder that
is not owned by `homeserver`.

**Moving an existing Tailscale server** is the delicate part. Started on an empty folder,
`tailscaled` is logged out and `tailscale-up` registers again: with a spent or expired auth key the
server falls off the tailnet (and whoever administers it over Tailscale is locked out), with a
valid one it comes back as a new machine with a new address, and the split-DNS nameserver in the
admin console points at the old one.

- A one-time step before `tailscaled` starts (`ExecStartPre`): if the new folder has no
  `tailscaled.state` and the old one has, copy the old folder over. Copy, not move, so a rollback
  to the previous generation still finds its state.
- NixOS restarts `tailscaled` with a single restart during a switch, so the copy happens between
  stop and start: same identity, a short blip.
- Test in the VM with a real tailnet: switch from the old to the new layout, `tailscale status`
  shows the same node id and address, no new machine in the admin console.

What changes for the operator, both networks:

| Action | Effect |
|--------|--------|
| `neo restore` on a new machine | It is the same device in the network: same address, same name, DNS entries stay valid. No new key needed |
| Old and restored machine both running | Two machines with one identity fight over the connection. RESTORE.md gets a line: switch the old one off first. This includes test restores of a production backup |
| Restore an old snapshot of this folder | Works as long as the key in it is still valid. Tailscale keys expire (180 days unless disabled in the admin console): then log in again. NetBird keys from a setup key do not expire |
| Clear appdata | The server leaves the network and needs a new key. `notes` say so |
| Backup in `files` format on someone else's computer | Whoever can read it can appear as this server in the network. Not new in kind (appdata already holds every app's data), but new for the network: `notes` and the backup guide recommend `encrypted` |

Open point: Tailscale can seal its state with the machine's TPM. Sealed state cannot move to
another machine, so a restore would log in again. Check what 1.98 does by default.

This is its own commit, independent of NetBird.

## 5. Control plane: `neo.services.netbird-server`

Category Network. Tagline "Your own private network between your devices". `replaces = [
"Tailscale" "ZeroTier" ]`. Licence AGPL-3.0.

### 5.1 Containers

| Container | Image | Network | Ports |
|-----------|-------|---------|-------|
| `netbird-server` | `netbirdio/netbird-server:<pinned>` | `internal` | `3478/udp` published and opened |
| `netbird-dashboard` | `netbirdio/dashboard:<pinned>` | `internal` | none |

- `config.yaml` is a `neo.core.secrets.templates` file mounted read-only. It holds
  `exposedAddress = https://<subdomain>.<domain>:443`, `auth.issuer = …/oauth2`, the redirect URIs,
  `store.engine = sqlite`, `reverseProxy.trustedHTTPProxies` = the `internal` Docker subnet, and
  the three secrets.
- Data: `${appdata}/netbird-server` → `/var/lib/netbird` (`mkAppdata`). Four SQLite files. Covered
  by snapshots and backup. `notes`: Clear appdata deletes the whole network, every device has to
  join again.
- Dashboard settings are plain environment (addresses, client id `netbird-dashboard`, no secret).
- Both tags pinned together like Authentik's, with a comment: read the release notes; the dashboard
  version belongs to the server version. The spike ran `netbird-server:0.82.1` with
  `dashboard:v2.96.1`.
- Without `reverseProxy.trustedPeers` the server logs that it trusts every address
  (`0.0.0.0/0`). Phase 2 reads what the setting gates and sets it.

### 5.2 Options

| Option | Rank | Notes |
|--------|------|-------|
| `enabled` | 0 | "Your own private network between your devices, with no outside account" |
| `ownerEmail` | 10 | First administrator. Label "Administrator e-mail" |
| `ownerPassword` | 20 | Secret, Generate. "Changing it here later does not change the login" |
| `relaySecret` | 30 | Secret, Generate. Changing it disconnects devices until they reconnect |
| `dataKey` | 40 | Secret, Generate (32 bytes base64, new helper). Encrypts setup keys and tokens in the database; losing it means creating them again |
| `sessionKey` | 50 | Secret, Generate. Changing it signs everyone out of the dashboard |
| `enrolThisServer` | 60 | Default on. §5.4 |
| `networkRange` | 70 | Label "Network address range". Default `10.213.0.0/16`: outside Tailscale's block and unlikely in a home LAN. Written by the bootstrap on the first start only. Later it is changed in the dashboard and here together, and every device gets a new address (§4.2). The client on this machine takes its `networkRange` from here |
| `subdomain` / `ingress` / `customDomains` | 100+ | `mkReverseProxyOptions` with `subdomain = "netbird"`, `auth.available = false` (clients, API and the dashboard's own login cannot sit behind tinyauth, as with Authentik) |
| `stunPort` | advanced | 3478 |

Secrets stay visible, per AGENTS.md.

### 5.3 SWAG vhost and the open `/setup`

`swag.nix` builds the server block with `mkSwagServer` and four kinds of locations, all to
`netbird-server:80` except the last:

| Path | Kind | Notes |
|------|------|-------|
| `/relay`, `/ws-proxy/` | WebSocket | `swagUpstream { proxyConf = false; }` with its own headers and a one-day read timeout. Including `proxy.conf` and setting the timeout again is the SWAG trap that kills every vhost |
| `/signalexchange.SignalExchange/`, `/management.ManagementService/`, `/management.ProxyService/` | gRPC | `grpc_pass` to a variable (request-time DNS, so SWAG starts while the container is down), `grpc_read_timeout`/`grpc_send_timeout` one day. These are not `proxy_*` directives, so no clash. `ProxyService` is only used by a NetBird reverse proxy; it is passed so one can be attached later (§7) |
| `/api/`, `/oauth2/` | HTTP | |
| `/` | HTTP | → `netbird-dashboard:80` |

Server level: `client_header_timeout` and `client_body_timeout` one day (long-lived streams).

Spike 0.1 ran exactly this block (now `swag.nix`): a client 0.71.4 on the
host and clients 0.82.1 in containers joined, got their network map, connected to each other
directly and over the relay, and the dashboard login worked. The same on all three paths: SWAG's
plain port, its PROXY-protocol port, and from the Internet through the VPS and rathole.

- **`/setup`**: until an owner exists, anyone who reaches the address can become it. The vhost
  answers 404 on `= /api/setup` for good; the bootstrap unit creates the owner over the internal
  network. So there is no window, also not on the first boot. Verified, and needed: within an hour
  of the name getting its certificate, a scanner fetched `/api/instance` (which says whether setup
  is still open).
- **Country filter**: a phone in a blocked country would lose the whole network, not one app. The
  vhost is built with `geo = false` and includes the filter only in `/`, `/api/` and `/oauth2/`.
  Device traffic (gRPC, relay) authenticates with keys and stays reachable. Check that
  `geo-access.conf` is valid inside a location (phase 2).
- **Auto-ban**: the `nginx-unauthorized` jail counts 401s. A dashboard tab with an expired session
  must not get its address banned and with it every device behind that address. Test in phase 2,
  and exclude the vhost from the jail if it does.
- **"Reachable from"**: this is the one address that has to stay open to the Internet, or phones
  and other servers cannot join or reconnect from outside. Default all; `notes` say what unticking
  Internet means.
- Health for the smoke test: `GET /api/instance` (no login, says whether setup is needed).
- `swag/check.nix` learns `grpc_pass` with a literal hostname, the same failure as `proxy_pass`.

### 5.4 Bootstrap: owner, and this server joins by itself

`netbird-server-setup` (`lib.neo.mkSetupService`, script `bootstrap.sh`, marker file in appdata so
it runs once). Steps 1 to 5 ran in the spike as a script against the container's internal address:

1. Wait for `/api/instance` on the container.
2. `setup_required` → `POST /api/setup` with `ownerEmail` and `ownerPassword` (read from a secret
   template, never from the unit's environment) and `create_pat` with `pat_expire_in = 1` (day).
   The server runs with `NB_SETUP_PAT_ENABLED=true` for that. The answer holds the token.
3. Network settings: read the account, change `network_range` and set `ipv6_enabled_groups` to
   empty (§4.1), write the **whole** settings object back. The API replaces the object; a partial
   one is refused or resets the rest.
4. With `enrolThisServer` and the `netbird` client on: create a one-off setup key (one day) and
   write it root-only where the client's login unit reads it. Wait until the client reports
   connected, read its address.
5. With "App names over NetBird" on: create the zone for this server's domain with the two A
   records (§4.3).
6. Delete the token. Nothing long-lived stays on disk; later changes are made in the dashboard.

- A failed step fails the unit, and `mkSetupService` retries. There is no second way: the
  documented `server.auth.owner` block in `config.yaml` creates the owner (from a password hash)
  but hands out no token, so steps 3 to 5 could not follow.
- On the same machine the client reaches `https://netbird.<domain>` like any device: a VPS reaches
  its own public address, at home the router needs hairpin NAT. The planned hosts entry to loopback
  was built and removed again: NetBird's resolver serves `/etc/hosts` to the containers as well,
  and `127.0.0.1` is the container itself there.
- Step 4 as built: the bootstrap creates the one-off key and runs `netbird up` itself, with the
  device name, and waits for the address.
- **Keys for the other servers** are made in the dashboard. A setup key can be created without an
  expiry (`expires_in = 0`), reusable or one-off.
- **If Neo should manage the network later** (zones for every server from `settings.toml`): a
  service user's token lives 365 days at most, and such a token can create its successor and
  delete itself. Tested; a timer could renew it. Not planned now.

### 5.5 Where to run it

STUN tells a device its public address, so the STUN server must see the device's real address on a
directly reachable UDP port. A TCP tunnel cannot provide that.

| Setup | Result |
|-------|--------|
| Server with a public IP, or home with port forwards for 443 and UDP 3478 | Everything works: direct connections between devices |
| Home behind CGNAT, public only through rathole/streamproxy | Devices can join, but every connection is relayed through the VPS and the home line. Slow |
| **A Neo on the VPS (next to streamproxy) runs `netbird-server`**, the home servers run only the client | **The layout this plan is built and tested for.** The data of the control plane then lives on the VPS |

`notes` and the guide say this; the service cannot detect it reliably, so no assertion. A warning
when `rathole` is enabled on the same machine points at the table.

On the VPS the control plane's own address goes Internet → streamproxy → SWAG's PROXY-protocol
port, like every other name there. gRPC and the relay WebSocket work on that path (spike 0.1, with
rathole in between as well). UDP 3478 is published by Docker directly and does not pass
streamproxy; the STUN server saw the real source address of a client outside Docker.

Seen in the spike, for `notes`:

- NetBird connects devices lazily by default: the first packet to a device that was idle is lost
  while the connection comes up.
- After a reboot of the hosting machine the devices reconnected by themselves.

## 6. The goal: every server in one network, closed to the Internet

The layout: a Neo on the VPS hosts the network (and is the public entry for the homes, as today).
`dev.example.com`, `prod.example.com` and the others join it.

| Step | Where | With this plan |
|------|-------|----------------|
| 1. The VPS hosts the network | `netbird-server` + `netbird` on it. It joins itself | Phase 2 |
| 2. A setup key for the other servers | Dashboard on the VPS: one reusable key, or one per server | By hand, once |
| 3. Every other server joins | `[services.netbird]` `enabled`, `managementUrl = "https://netbird.<vps domain>"`, `setupKey` | Phase 1 |
| 4. Phones and laptops join | NetBird app, the same address, a login made in the dashboard | Guide |
| 5. App names resolve to NetBird addresses | One zone per server in the dashboard; the VPS's own is made by the bootstrap. "App names over NetBird" on each server (§4.3) | Phase 1 and 2 |
| 6. Close the apps | See below | Works today |

Two ways to close a server, both leave certificates working:

| Server | Setting | To the Internet |
|--------|---------|-----------------|
| Behind the VPS (rathole) | "Certificates only" on the tunnel (`rathole.certificateOnly`). It needs private names, and "App names over NetBird" now counts for that | No HTTPS at all: the VPS has nowhere to send port 443 for this server. Port 80 stays for certificate renewal |
| Reachable directly, and the VPS's own apps | Untick "Internet" in "Reachable from", per service | The 404 page |

"Closed" means this, by decision: the apps do not answer the Internet. Ports 80 and 443 stay
reachable where SWAG needs them for certificates (HTTP-01).

What stays true afterwards:

- **The network's own address stays public.** `netbird.<vps domain>` plus UDP 3478. Existing
  connections survive when the VPS is down; new ones and roaming phones wait for it. Keep "Internet"
  ticked on that one service.
- **A second way in.** With everything closed, a broken NetBird means the LAN or the console.
  Tailscale next to it (§4.2) is such a second way while moving over.
- **Everyone in the network reaches every port of every server**, also ports the server's firewall
  keeps closed elsewhere (SSH, the web UI): the client accepts everything on `wt0` and filters only
  by the network's access rules, and NetBird's default rule is all to all. Narrow it in the
  dashboard. `notes` and the guide say this. Tailscale does the same with its own firewall chain.
- **Apps on another server, from a container**, go over the public path on the hosting machine
  (§4.4). An app there that calls a closed app on another server does not reach it by name.

## 7. NetBird's ways of publishing: not built, not blocked

Neither is needed for §6, and none of it is planned. What keeps the door open:

| What someone may want | What Neo does for it | What they do themselves |
|-----------------------|----------------------|-------------------------|
| NetBird reverse proxy (beta) in front of apps | The control plane's vhost passes `ProxyService` (§5.3). Nothing claims the proxy's ports | Run the proxy container on a public machine, with its own domain; point services at the Neo server's port 443 |
| `netbird expose <port>` | The client is installed, the skill explains it | Needs a proxy in the network |
| NetBird instead of rathole between VPS and home | Nothing yet. It would be a second transport in `streamproxy` (send to the home server's NetBird address) plus a setting on the home side naming the VPS | — |

One rule for all three, in the guide, the skill and the service notes: visitors that come in through
a NetBird proxy arrive at SWAG from a NetBird address. They count as "NetBird", not as "Internet".
A service that is open to NetBird and pointed at by a proxy is public, whatever its Internet tick
says. tinyauth still asks for a login. If the tunnel variant is ever built, the VPS's address has to
be classed as Internet for that reason.

Why the proxy is not Neo's public entry: beta; in HTTP mode it ends TLS on the public machine (today
the VPS never sees plaintext) and duplicates SWAG and tinyauth; in passthrough mode it wants port
80, which SWAG needs for certificates.

## 8. Guides, skills, docs

- Guide `private-traffic-netbird` (services `netbird`, optional `netbird-server`): join, install
  the app on a phone, app names with the zone in the dashboard (a `neo-check` block resolves a name
  and compares it with the server's NetBird address), the two ways to close a server (§6). Mirrors
  `private-traffic-tailscale`.
- Guide `own-private-network` (services `netbird-server`, `netbird`): where to run it (§5.5), DNS
  and the UDP port, first login, add a server, add a phone, routes and exit nodes in the dashboard.
  This is §6 as a guide.
- `reach-from-internet`: the new "Reachable from" choice.
- `adblock-at-home`: unchanged. Pi-hole over NetBird is possible (§4.3) but not part of this plan.
- Hermes skills `neo-netbird`, `neo-netbird-server` via `mkServiceSkill`; credentials only real
  settings keys.
- Privacy map: no tag. Tailscale has none either and no category fits a private network.
- Every doc and agent skill that lists `local / tailscale / web` changes in the same commit as the
  code.

## 9. Phases

Each phase ends with `just launch`, the checks named, `just format && just check`.

**Phase 0: spikes in the VM.** Done on 2026-11-10; results in §10.

**IPv6 source addresses at SWAG.** Done, 08920a4.

**Phase 1: client.** Done, 546d9fc. Tested in the VM against a control plane started by hand:
join with a setup key and device name, state in appdata and the same address after a reboot,
`neo-host-dns` waits for NetBird's resolver so containers resolve app names after a boot,
`netbird-check` fails with the missing zone and the value to fix and passes once it exists, a
second NetBird device opens an app by name and SWAG classes it `netbird`, an app with NetBird
unticked answers 404 there while the LAN still works, and every new assertion fires (both DNS
switches, range in `100.64.0.0/10` next to Tailscale, range in Docker's space, Pi-hole without its
home address, `certificateOnly` with and without app names).

**Phase 2: control plane.** Done, de1af06. Tested in the VM with all NetBird state wiped first:
the bootstrap created the administrator, set the range and IPv6 off, joined the server and created
the zone, then deleted its token; `/api/instance` 200 and setup not required from the Internet,
`/api/setup` 404; the owner signs in through the built-in login; a second device joined through
the public address (VPS, rathole) and opened an app that answers 404 to the Internet; no SWAG
`emerg`; a second boot changed nothing. Not run: streamproxy on the same machine (the PROXY-protocol
path was tested through the VPS and rathole instead), the dashboard in a browser.

**Tailscale state in appdata.** Dropped (decided, §4.5).

**Later, each ends in a decision:** Authentik as login provider, the NetBird tunnel for
streamproxy, Pi-hole for the NetBird network, zones for every server from `settings.toml`.

## 10. What phase 0 showed

Setup: the dev VM (`just launch`), `netbird-server:0.82.1` and `dashboard:v2.96.1` started by hand
on the `internal` network behind the draft vhost, the nixpkgs client 0.71.4 on the guest, two
clients 0.82.1 in containers (one through the public path, one direct), Tailscale 1.98 against a
Headscale on the guest. The control plane was reachable from the Internet through the VPS and
rathole.

| # | Question | Result |
|---|----------|--------|
| 0.1 | Client 0.71.4 from nixpkgs against the current server | Works: join, network map, direct and relayed connections, DNS zones |
| 0.1 | gRPC and the relay WebSocket through SWAG, its PROXY-protocol port, and rathole | Work on all three with the vhost of §5.3. In 20 idle minutes no client lost its management or signal stream, on any path |
| 0.1 | Docker keeps the source address on the STUN port | Yes for clients outside Docker. Not through rathole (TCP only), as expected |
| 0.1 | `/api/setup` closed at the vhost | 404 from outside, reachable for the bootstrap over the internal address |
| 0.2 | Owner and token without a browser | Works with `NB_SETUP_PAT_ENABLED=true`. `server.auth.owner` is no substitute (no token) |
| 0.2 | Range outside `100.64.0.0/10` over the API | Accepted, as a whole-object write. A change with devices joined re-addresses them at once |
| 0.2 | IPv6 addresses off for the network | `ipv6_enabled_groups = []` in the same write: every device drops its IPv6 address at once and device names resolve to IPv4 only (0.71.4 and 0.82.1) |
| 0.2 | Setup keys, tokens | Setup keys may never expire. Service-user tokens: 365 days at most, can renew themselves |
| 0.3 | dnsmasq next to NetBird's resolver on `wt0` | They collide on port 53. Not needed: zones in the control plane do the job (§4.3) |
| 0.3 | Sibling zones `dev.` / `prod.`, the rest of the domain still public | Yes, on 0.71.4 and 0.82.1 |
| 0.3 | Pi-hole on the NetBird address | Works with a stand-in DNS server once NetBird's resolver is moved. Not part of the plan |
| 0.4 | Resolvconf key, container DNS | Key `wt0`. Containers keep the resolver the host had when they started (§4.4) |
| 0.5 | Tailscale and NetBird on one host | Run together. Two rules: range outside `100.64.0.0/10`, one DNS owner (§4.2). **Decides §4.1: fourth value `netbird`** |

**Found on the way**

| Finding | Consequence |
|---------|-------------|
| Every IPv6 connection to SWAG's port arrives as `172.18.0.1` (Docker's proxy, the `internal` network has no IPv6) and counts as "Home network", also from the Internet on a server with a public IPv6 address. The IPv6 entries in the class maps can never match. With streamproxy or rathole in front the PROXY-protocol port carries the real address, so the VPS layout is not affected | Its own fix, before or with phase 1: IPv6 on the `internal` network, or SWAG's ports without Docker's proxy. Until then NetBird stays on IPv4 (§4.1, §5.4) |
| On `wt0` the host firewall does not apply (§6) | `notes`, guide |
| A scanner asked `/api/instance` within the hour | The 404 on `/api/setup` is needed from the first second |
| With a control plane that sends a search domain but no nameserver, "Use Tailscale DNS" leaves the host without any resolver | Exists today, independent of NetBird. `neo-host-dns` goes on after 90 s with an empty resolver; worth a message |
| Zone records and containers' resolvers hold NetBird addresses | Set the range before devices join (§4.2) |

**Still open**

| Point | Status |
|-------|--------|
| `geo-access.conf` inside a location | nginx loads it (no `emerg`); the country filter itself was not exercised |
| The `nginx-unauthorized` jail and the dashboard's 401s | Not tested |
| Whether NetBird's resolver follows a change of the upstream nameserver (DHCP) without a restart | Not tested |
| What `reverseProxy.trustedPeers` gates | Set to the network range, which removes the `0.0.0.0/0` default; the effect is not verified |
| Dashboard login in a browser | The same login was run with curl (PKCE code flow, owner password); the browser page itself was not opened |
| Streamproxy and the control plane on one machine | Not run in the VM |

## 11. Decided

| Topic | Decision |
|-------|----------|
| Ingress | Fourth value `netbird`, next to `tailscale`. Both networks run on one server (§4.1, §4.2) |
| Range | `10.213.0.0/16` by default, the same on `netbird-server` and the client. Outside `100.64.0.0/10` when Tailscale runs too |
| Host DNS | One owner: "Use Tailscale DNS" or "Use NetBird DNS", not both |
| App names | Zones in the control plane, one per server. No dnsmasq for NetBird, and Tailscale's split DNS stays as it is. This replaces the earlier "one neutral `split-dns` for both" |
| IPv6 in the network | Off for a network hosted by Neo. SWAG keeps IPv6 source addresses now, but NetBird's IPv6 range is random per network and not in the class map |
| IPv6 at SWAG | Own IPv6-only network for SWAG; home devices on a public IPv6 address count as Internet |
| Client state | In appdata for NetBird. Tailscale stays as it is, TPM sealing kept (§4.5) |
| NetBird as public entry | Not built, not blocked (§7) |
| PROXY-protocol port | Fixed separately (4a2476f) |
| "Closed to the Internet" | The apps answer 404 there, or nothing with "Certificates only". Ports for certificates stay (§6) |
| Domains | One registered domain, each server its own zone under it (`dev.…`, `prod.…`) |
| Closing a server | Per service, or "Certificates only" behind the VPS. No server-wide setting |
| Host of the network | A Neo on the VPS (§5.5) |
| Identity in backups | Accepted, also for `files` backups. The notes still recommend `encrypted` |
| Client version | 0.71.4 from nixpkgs 26.05. No gap with server 0.82.1 found |

Open: the points at the end of §10.

## Not planned

- NetBird's reverse proxy as Neo's public entry, and the NetBird tunnel for streamproxy (§7).
- Managing groups, access rules, routes, exit nodes or other servers' zones from `settings.toml`.
  They stay in the dashboard; Neo only does the one-time bootstrap.
- Certificates over DNS-01, so that a server needs no reachable port at all.
- A server-wide "Reachable from" that caps every service.
- App names for several servers that share the *same* Neo domain (one record per app name instead
  of the wildcard).
- A DNS server of Neo's on the NetBird address (Pi-hole for the network).
- PostgreSQL for the control plane, the legacy four-service layout, coturn.

## Appendix: the vhost

The vhost that was tested in phase 0 is now `nix/services/netbird-server/swag.nix`, with the
country filter moved into the `/`, `/api/` and `/oauth2/` locations.
