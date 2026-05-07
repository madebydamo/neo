# Immich-drop service options.
{...}: {
  flake.modules.nixos.immich-drop-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.immich-drop = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption ("immich-drop service");
              additionalMountPoints = mkOption {
                type = types.attrsOf types.str;
                default = {};
                description = "Additional volume mounts";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "drop";
              auth.enabled = false;
            };
        };
        default = {};
        description = "Immich-drop service configuration";
      };
    };
}
