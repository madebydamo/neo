# Hermes skill for streamproxy.
{...}: {
  flake.modules.nixos.streamproxy-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.streamproxy.skill.conf = lib.neo.mkServiceSkill {
      service = "streamproxy";
      inherit config;
      description = "Public IP sharing: nginx stream SNI + rathole server";
      tags = ["neo" "streamproxy" "edge"];
      title = "Neo · Streamproxy";
      body = ''
        ## When to Use
        Shared public IP, SNI routing, rathole tunnels from remote homeservers.

        ## Architecture
        - Host nginx stream + rathole server
        - HTTPS stream uses `proxy_protocol on` toward backends
        - Local SWAG domains → host `localHttpsProxyProtocolPort` (9982), not plain 443. SWAG binds it on loopback; `streamproxy-swag-pp` passes it on from the container's host address
        - Rathole entry ports → remote client must bind HTTPS to SWAG PROXY-protocol port
        - Entries map domains/tokens to tunnel ports
        - Routing order (nginx, same for 443 SNI and 80 Host): exact name, then the longest `*.` wildcard. Tunnels win over the local SWAG for the same name, so a tunnel `*.dev1.example.com` beats a local `*.example.com`

        ## Pitfalls
        - Mismatched PROXY protocol (sender vs SWAG listener) breaks that path only; LAN 443 stays fine
        - Intermediate streamproxy hops must terminate rathole on SWAG PP port, not another plain-443 streamproxy, if real client IPs are required
      '';
    };
  };
}
