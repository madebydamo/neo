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
          options = {
            enabled = mkEnableOption (lib.mdDoc "immich-drop service");
            subdomain = mkOption {
              type = types.nullOr types.str;
              default = "drop";
              description = lib.mdDoc "Subdomains for the service";
            };
            proxyConf = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = lib.mdDoc "Nginx proxy conf for swag";
            };
            additionalMountPoints = mkOption {
              type = types.attrsOf types.str;
              default = {};
              description = lib.mdDoc "Additional volume mounts";
            };
          };
        };
        default = {};
        description = lib.mdDoc "Immich-drop service configuration";
      };
    };
}
