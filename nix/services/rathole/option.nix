# Rathole client service options.
{...}: {
  flake.modules.nixos.rathole-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.rathole = mkOption {
        type = types.submodule {
          options = {
            enabled = mkEnableOption ("rathole client service");
            token = mkOption {
              type = types.str;
              description = "Authentication token for rathole";
            };
            remoteAddr = mkOption {
              type = types.str;
              description = "Remote server address for rathole";
            };
            port = mkOption {
              type = types.port;
              default = 2333;
              description = "Remote server port for rathole";
            };
            name = mkOption {
              type = types.str;
              description = "Name prefix for rathole services";
            };
          };
        };
        default = {};
        description = "Rathole client configuration";
      };
    };
}
