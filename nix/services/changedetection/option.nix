# Changedetection service options.
# Web UI protected with tinyauth forward auth (enabled by default).
{...}: {
  flake.modules.nixos.changedetection-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.changedetection = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption (lib.mdDoc "changedetection.io website change detection service");
              port = mkOption {
                type = types.port;
                default = 5000;
                description = lib.mdDoc "Internal port for changedetection web UI";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "changedetection";
            };
        };
        default = {};
        description = lib.mdDoc "Changedetection service configuration";
      };
    };
}
