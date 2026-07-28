# Hermes skill for streamproxy.
{...}: {
  flake.modules.nixos.streamproxy-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.streamproxy;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.streamproxy.skill.conf = lib.neo.mkServiceSkill {
      service = "streamproxy";
      inherit cfg domain;
      description = "Public IP sharing: nginx stream SNI + rathole server";
      tags = ["neo" "streamproxy" "edge"];
      title = "Neo · Streamproxy";
      body = ''
        ## When to Use
        Shared public IP, SNI routing, rathole tunnels from remote homeservers.

        ## Architecture
        - Host nginx stream + rathole server
        - HTTPS stream uses `proxy_protocol on` toward backends
        - Local SWAG domains → host `localHttpsProxyProtocolPort` (9982), not plain 443
        - Rathole entry ports → remote client must bind HTTPS to SWAG PROXY-protocol port
        - Entries map domains/tokens to tunnel ports

        ## Pitfalls
        - Mismatched PROXY protocol (sender vs SWAG listener) breaks that path only; LAN 443 stays fine
        - Intermediate streamproxy hops must terminate rathole on SWAG PP port, not another plain-443 streamproxy, if real client IPs are required
      '';
    };
  };
}
