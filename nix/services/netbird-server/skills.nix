# Hermes skill for the NetBird control plane.
{...}: {
  flake.modules.nixos.netbird-server-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.netbird-server.skill.conf = lib.neo.mkServiceSkill {
      service = "netbird-server";
      inherit config;
      description = "NetBird control plane (management, signal, relay, STUN) and dashboard";
      tags = ["neo" "netbird" "vpn"];
      body = ''
        ## When to Use
        Running the private network itself: dashboard login, adding devices and servers, setup keys, DNS zones for app names, why devices are relayed.

        ## Layout
        - Containers `netbird-server` (combined server) and `netbird-dashboard`, behind SWAG at `https://<subdomain>.<domain>`
        - UDP STUN port (`stunPort`, 3478) published directly; it cannot pass rathole or streamproxy
        - Data: `appdata/netbird-server/data` (SQLite). Clear appdata deletes the network
        - `netbird-server-setup` ran once (marker `appdata/netbird-server/bootstrap-done`): administrator, `networkRange`, IPv6 off, this server joined, DNS zone for the app names. Later changes: dashboard

        ## CLI
        ```bash
        systemctl status docker-netbird-server docker-netbird-dashboard netbird-server-setup
        journalctl -u netbird-server-setup -b
        docker logs netbird-server 2>&1 | tail -50
        curl -s https://<subdomain>.<domain>/api/instance    # {"setup_required":false}
        ```
        `/api/setup` answers 404 at SWAG on purpose.

        ## Credentials
        - `ownerEmail`, `ownerPassword`: first administrator; the password is only used when the network is created
        - `relaySecret`, `dataKey`, `sessionKey`: generated secrets; changing `dataKey` invalidates stored setup keys and tokens

        ## Adding a server or phone
        Dashboard → Setup Keys → create (one-off or reusable). On another Neo: NetBird on, `setupKey`, `managementUrl = "https://<subdomain>.<domain>"`. App names: Dashboard → DNS → Zones → zone `<that server's domain>`, A records `*` and `@` → its NetBird address.

        ## Pitfalls
        - Behind rathole only: devices join but every connection is relayed (no direct UDP)
        - Keep Internet ticked under Reachable from for this service
        - The country filter is not applied to device traffic (gRPC, relay), only to the dashboard, login and API
      '';
    };
  };
}
