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
          # rtcsync tells the kernel the clock is synchronised (node exporter's
          # node_timex_sync_status, the ISO "Clocks in sync" panel); it replaces
          # chrony's RTC trimming.
          enableRTCTrimming = false;
          extraConfig = ''
            allow
            rtcsync
          '';
        };

        networking.firewall.allowedUDPPorts = [123];
      };
    };
}
