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
              enabled = mkEnableOption (lib.mdDoc "syncthing service");
              additionalMountPoints = mkOption {
                type = types.attrsOf types.str;
                default = {};
                description = lib.mdDoc "Additional volume mounts";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "syncthing";
              auth.enabled = true;
            };
        };
        default = {};
        description = lib.mdDoc "Syncthing service configuration";
      };
    };
}
