# Hermes skill for pihole.
{...}: {
  flake.modules.nixos.pihole-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.pihole;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.pihole.skill.conf = lib.neo.mkServiceSkill {
      service = "pihole";
      inherit cfg domain;
      description = "Pi-hole DNS, blocklists, web admin password";
      tags = ["neo" "pihole" "dns"];
      body = ''
        ## When to Use
        DNS filtering, local DNS for Neo domains, web admin, upstream DNS.

        ## CLI extras
        ```bash
        docker exec -it pihole pihole status
        docker exec -it pihole pihole -q example.com
        ```

        ## Credentials
        - Neo: `services.pihole.webPassword` (admin UI password)
        - Edge: tinyauth in addition

        ## Procedures
        1. Health + `pihole status`
        2. Local DNS records for services often managed by Neo modules — prefer settings over ad-hoc edits that get overwritten
        3. Upstream DNS via options

        ## Pitfalls
        - Breaking Pi-hole DNS can break local name resolution for the whole LAN if clients use it

        ## Verification
        - DNS query works; admin UI login with webPassword
      '';
    };
  };
}
