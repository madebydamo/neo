# Hermes skill for rathole.
{...}: {
  flake.modules.nixos.rathole-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.rathole;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.rathole.skill.conf = lib.neo.mkServiceSkill {
      service = "rathole";
      inherit cfg domain;
      description = "Rathole client tunnel to public edge";
      tags = ["neo" "rathole" "tunnel"];
      body = ''
        ## When to Use
        Homeserver without public IP: tunnel 80/443 to streamproxy/remote.

        ## Architecture notes
        - Client service with token, remoteAddr, ports
        - Pairs with streamproxy on public machine
        - `certificateOnly`: HTTP tunnel only (Let's Encrypt); no public HTTPS

        ## Credentials
        - `services.rathole.token` must match server entry

        ## Pitfalls
        - Token mismatch = silent connection failure
        - `certificateOnly` needs Pi-hole with `localIP` or Tailscale split DNS

        ## Verification
        - Tunnel up; public HTTPS reaches local SWAG unless `certificateOnly`
      '';
    };
  };
}
