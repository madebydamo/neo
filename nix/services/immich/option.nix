# Immich service options.
{...}: {
  flake.modules.nixos.immich-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.immich = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption (lib.mdDoc "immich service");
              additionalMountPoints = mkOption {
                type = types.attrsOf types.str;
                default = {};
                description = lib.mdDoc "Additional volume mounts";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "immich";
              auth.publicPaths = [
                "^\\/share\\/"
                "^\\/api\\/"
              ];
            };
        };
        default = {};
        description = lib.mdDoc "Immich service configuration";
      };
    };
}
