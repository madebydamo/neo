# Hermes skill for swag.
{...}: {
  flake.modules.nixos.swag-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.swag;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.swag.skill.conf = lib.neo.mkServiceSkill {
      service = "swag";
      inherit cfg domain;
      description = "SWAG reverse proxy, TLS, proxy-confs, certificates, dashboard, geo";
      tags = ["neo" "swag" "nginx" "tls" "dashboard" "goaccess" "geoip" "dbip"];
      title = "Neo · SWAG (edge reverse proxy)";
      body = ''
        ## When to Use
        TLS, reverse proxy routing, certs, SWAG Dashboard / GoAccess, geo allow-deny.

        ## Architecture
        - Proxy confs: `appdata/swag/nginx/proxy-confs/<sub>.subdomain.conf`
        - Dual HTTPS listeners (`listen-https.conf`):
          - **443** plain TLS — LAN / direct
          - **8443** TLS + PROXY protocol — streamproxy / rathole (host port `localHttpsProxyProtocolPort`, 9982 when streamproxy co-located)
        - Docker mods: swag-dashboard + swag-dbip
        - Geo lists: `services.swag.geo` (empty = unrestricted)
        - Real client IPs: only on the PROXY-protocol path; old access.log lines stay private-proxy IPs until new traffic

        ## Procedures
        1. Domain/email set; container healthy
        2. Routing: fix service `swag.nix`, not hand-edited confs
        3. Geo map empty: confirm streamproxy targets PP port with `proxy_protocol on`, rathole HTTPS → PP host port
        4. Dashboard: `https://swag.<domain>/` → tinyauth → UI

        ## Pitfalls
        - PROXY-protocol port is not for browsers; LAN must use 443
        - PreStart regenerates proxy-confs / dbip / listen-https
        - Do not mix swag-dbip with swag-maxmind
        - Never put a Docker/DNS hostname directly in proxy_pass: nginx
          resolves that at start; NXDOMAIN is `[emerg]` and takes every vhost
          down. Use `set $upstream_app` / `$upstream_port` / `$upstream_proto`
          (request-time DNS via resolver.conf). Missing backend → 502, nginx stays up.
        - Custom domains hairpin to `https://127.0.0.1:443` (SNI/Host = service
          subdomain). Do not proxy to the public hostname — Docker DNS cannot
          resolve it and that used to crash SWAG.
      '';
    };
  };
}
