# NTP server options using chrony.
{...}: {
  flake.modules.nixos.ntp-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.ntp = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "exact time for your homeserver and the devices on your home network" {rank = 0;};
              servers = mkOption {
                type = types.listOf types.str;
                default = [
                  "0.pool.ntp.org"
                  "1.pool.ntp.org"
                  "2.pool.ntp.org"
                  "3.pool.ntp.org"
                ];
                description = "Internet time servers your homeserver gets the exact time from. The defaults work worldwide.";
                rank = 10;
                ui = lib.neo.ui.mkUi {
                  label = "Time servers";
                  group = lib.neo.ui.groups.advanced;
                };
              };
            }
            # NixOS services.chrony runs as chronyd.service.
            // lib.neo.mkSystemdUnits ["chronyd"]
            // lib.neo.mkServiceMeta {
              category = "Network";
              icon = ./icon.svg;
              tagline = "Keeps the clocks in your home exact";
              description = ''
                Gives your server, and any device at home that asks for it, the exact time. Usually there is nothing to configure.
              '';
              notes = ''
                Chrony syncs from public NTP pools and serves time to the local network on UDP port 123. Keeps time steady even on flaky connections.
              '';
              projectUrl = "https://chrony-project.org/";
              githubUrl = "https://github.com/mlichvar/chrony";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "NTP server configuration";
      };
    };
}
