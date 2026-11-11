# Hermes skill for pihole.
{...}: {
  flake.modules.nixos.pihole-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.pihole.skill.conf = lib.neo.mkServiceSkill {
      service = "pihole";
      inherit config;
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
        - With `services.tailscale.splitDns`, `localIP` is required; Pi-hole then publishes LAN:53 only so Tailscale dnsmasq can bind the tailnet address
        - With NetBird's `services.netbird.acceptDns`, `localIP` is required too: NetBird's resolver binds 127.0.0.153:53

        ## Verification
        - DNS query works; admin UI login with webPassword
      '';
    };
  };
}
