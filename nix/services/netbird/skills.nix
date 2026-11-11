# Hermes skill for the NetBird client.
{...}: {
  flake.modules.nixos.netbird-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.netbird.skill.conf = lib.neo.mkServiceSkill {
      service = "netbird";
      inherit config;
      description = "NetBird client: this server in a private WireGuard network";
      tags = ["neo" "netbird" "vpn"];
      body = ''
        ## When to Use
        Joining the server to a NetBird network, app names over NetBird, "Reachable from: NetBird", routing and SSH over NetBird, checking why a device cannot reach an app.

        ## CLI
        ```bash
        netbird status            # Management / Signal / Relays, peers connected
        netbird status -d         # per peer: direct or relayed, latency
        ip -4 addr show wt0       # this server's NetBird address
        systemctl status netbird netbird-check
        journalctl -u netbird-check -b
        ```
        "Connected" with a relayed connection works but is slow: UDP to the control plane's STUN port (3478) or between the devices is blocked.

        ## Settings (services.netbird)
        - `setupKey`: from the dashboard (Setup Keys); only used until the server has joined
        - `managementUrl`: empty = NetBird server on this machine, else netbird.io
        - `networkRange`: must equal the network's range (dashboard → Settings → Networks). SWAG uses it for the "NetBird" class. `netbird-check` fails with the right value when it differs
        - `acceptDns` ("Use NetBird DNS"): not together with Tailscale's `acceptDns`. The resolver listens on 127.0.0.153
        - `appNames`: declares that the network answers this server's app names; needs a DNS zone in the dashboard
        - `ssh`, `routing`, `acceptRoutes`, `port`, `hostname` (first login only)
        Groups, access rules, routes, exit nodes and DNS zones live in the NetBird dashboard only.

        ## App names over NetBird
        In the dashboard: DNS → Zones → add the zone `<services.swag.domain>`, records `*` and `@` of type A → this server's NetBird address (`ip -4 addr show wt0`), distribution group All. Each Neo server with its own domain (dev.example.com, prod.example.com) gets its own zone. Then turn on `appNames`.
        ```bash
        dig +short @127.0.0.153 <subdomain>.<domain>   # must print the wt0 address
        ```

        ## Reachable from
        Requests from `networkRange` count as "NetBird". Apps whose `ingress` list was narrowed by hand do not list it: tick NetBird there. Everyone in the network reaches every port of this server (the client accepts all traffic on wt0); narrow it with access rules in the dashboard.

        ## netbird expose / NetBird reverse proxy
        Visitors through a NetBird proxy arrive from a NetBird address and count as "NetBird", not "Internet": an app open to NetBird behind such a proxy is public. tinyauth still asks for a login.

        ## Verification
        - `netbird status` shows Management and Signal connected
        - `systemctl is-active netbird-check` is active (exited 0)
        - From a NetBird device: `https://<app>.<domain>` opens the app; with NetBird unticked under Reachable from, the "not found" page
      '';
    };
  };
}
