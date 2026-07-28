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
      description = "SWAG reverse proxy, TLS, proxy-confs, certificates, dashboard";
      tags = ["neo" "swag" "nginx" "tls" "dashboard" "goaccess"];
      title = "Neo · SWAG (edge reverse proxy)";
      body = ''
        ## When to Use
        TLS, domains, reverse proxy routing, certificate issues, nginx conf for services,
        access-log analytics / SWAG Dashboard.

        ## Architecture notes
        - Generated proxy confs: `appdata/swag/nginx/proxy-confs/<subdomain>.subdomain.conf`
        - Each service's `swag.nix` sets `proxyConf`; SWAG preStart materializes them
        - Docker network: backends on `internal`; host services via `host.docker.internal`
        - SWAG is itself a proxied service (`mkReverseProxyOptions`, default subdomain `swag`):
          - docker-mod `linuxserver/mods:swag-dashboard` (GoAccess) when SWAG is enabled
          - UI: `https://swag.<domain>/` (override with `services.swag.subdomain`)
          - tinyauth via standard `services.swag.auth` (default on)
          - conf: `nix/services/swag/swag.nix` (local PHP root `/dashboard/www`, not an upstream app)
          - Stats from `appdata/swag/log/nginx`; clear by removing those logs and recreating the container
          - Geo graphs need MaxMind/DB-IP docker-mod (not enabled by Neo yet)

        ## Credentials
        - Options: `services.swag.domain`, `email`, `proxyPass`, `subdomain`, `auth`
        - Let's Encrypt uses the configured email; no API token in Neo

        ## Procedures
        1. Confirm domain/email set and container healthy
        2. For routing bugs: inspect generated proxy-conf, not hand-edit permanently
        3. Fix durable routing in the service's `swag.nix` / options, then activate
        4. Cert renew failures: check logs, DNS A/AAAA, ports 80/443 reachability
        5. Dashboard: confirm `DOCKER_MODS` includes swag-dashboard and `swag.subdomain.conf` exists

        ## Pitfalls
        - Hand-edited proxy-confs are wiped on container restart/preStart
        - streamproxy changes local ports (9980/9981) — do not assume 80/443 on host in that mode
        - The docker-mod may drop a `dashboard.subdomain.conf`; Neo removes it when subdomain ≠ dashboard

        ## Verification
        - HTTPS to a known subdomain works; cert valid; proxy-conf exists for enabled services
        - Dashboard: `https://swag.<domain>/` → 302 to tinyauth (if auth on), then UI after login
      '';
    };
  };
}
