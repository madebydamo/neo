# NTP server options using chrony.
{...}: {
  flake.modules.nixos.ntp-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.ntp = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "NTP server (chrony)";
              servers = mkOption {
                type = types.listOf types.str;
                default = [
                  "0.pool.ntp.org"
                  "1.pool.ntp.org"
                  "2.pool.ntp.org"
                  "3.pool.ntp.org"
                ];
                description = "Upstream NTP servers/pools to synchronize from.";
              };
            }
            // lib.neo.mkServiceMeta {
              icon = "🕒";
              description = ''
                Chrony is a versatile implementation of the Network Time Protocol (NTP) used for accurate system clock synchronization.
                In this homeserver it syncs from public NTP pools and serves time to other devices on the local network via UDP port 123.
                This provides low-latency time for the LAN, reduces reliance on external services for clients, and maintains precise time even on intermittent connections.
              '';
              projectUrl = "https://chrony-project.org/";
              githubUrl = "https://github.com/mlichvar/chrony";
            };
        };
        default = {};
        description = "NTP server configuration";
      };
    };
}
