# NTP server implementation using chrony.
{...}: {
  flake.modules.nixos.ntp = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.ntp;
    in {
      config = mkIf cfg.enabled {
        services.chrony = {
          enable = true;
          servers = cfg.servers;
          serverOption = "iburst";
          initstepslew = {
            enabled = true;
            threshold = 1000;
          };
          extraConfig = ''
            allow
          '';
        };

        networking.firewall.allowedUDPPorts = [123];
      };
    };
}
