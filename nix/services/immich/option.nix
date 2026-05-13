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
              enabled = mkEnableOption "immich service";
              additionalMountPoints = mkOption {
                type = types.attrsOf types.str;
                default = {};
                description = "Additional volume mounts";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "immich";
              auth.publicPaths = [
                "^/share/"
                "^/.well-known/immich"
                "^/api/"
              ];
            };
        };
        default = {};
        description = "Immich service configuration";
      };
    };
}
