# Hermes skill for rathole.
{...}: {
  flake.modules.nixos.rathole-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.rathole.skill.conf = lib.neo.mkServiceSkill {
      service = "rathole";
      inherit config;
      description = "Rathole client tunnel to public edge";
      tags = ["neo" "rathole" "tunnel"];
      body = ''
        ## When to Use
        Homeserver without public IP: tunnel 80/443 to streamproxy/remote.

        ## Architecture notes
        - Client service with token, remoteAddr, ports
        - Pairs with streamproxy on public machine
        - `certificateOnly`: HTTP tunnel only (Let's Encrypt); no public HTTPS (global)
        - Per-service `ingress` (local / tailscale / netbird / web) gates the SWAG vhost; rathole itself stays a shared tunnel

        ## Credentials
        - `services.rathole.token` must match server entry

        ## Pitfalls
        - Token mismatch = silent connection failure
        - `certificateOnly` needs Pi-hole with `localIP`, Tailscale split DNS or NetBird `appNames`

        ## Verification
        - Tunnel up; public HTTPS reaches local SWAG unless `certificateOnly`
      '';
    };
  };
}
