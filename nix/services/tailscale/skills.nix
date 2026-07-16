# Hermes skill for tailscale.
{...}: {
  flake.modules.nixos.tailscale-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.tailscale;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.tailscale.skill.conf = lib.neo.mkServiceSkill {
      service = "tailscale";
      inherit cfg domain;
      description = "Tailscale mesh VPN and tailscaled";
      tags = ["neo" "tailscale" "vpn"];
      body = ''
        ## When to Use
        Mesh access, exit node, advertise routes, SSH over tailnet.

        ## CLI extras
        ```bash
        tailscale status
        tailscale ip -4
        ```

        ## Credentials
        - `services.tailscale.authKey` (reusable/ephemeral keys from admin console)
        - Treat authKey as secret

        ## Verification
        - `tailscale status` shows connected; ping tailnet IP
      '';
    };
  };
}
