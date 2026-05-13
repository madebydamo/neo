# Syncthing service options.
{...}: {
  flake.modules.nixos.syncthing-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.syncthing = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "syncthing service";
              additionalMountPoints = mkOption {
                type = types.attrsOf types.str;
                default = {};
                description = "Additional volume mounts";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "syncthing";
              auth.enabled = true;
            };
        };
        default = {};
        description = "Syncthing service configuration";
      };
    };
}
