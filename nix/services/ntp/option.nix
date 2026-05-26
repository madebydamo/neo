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
          options = {
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
          };
        };
        default = {};
        description = "NTP server configuration";
      };
    };
}
