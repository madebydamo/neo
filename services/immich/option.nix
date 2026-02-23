{
  config,
  lib,
  ...
}:
with lib; {
  options.neo.services.immich = mkOption {
    type = types.submodule {
      options = {
        enabled = mkEnableOption (lib.mdDoc "immich service");
        subdomain = mkOption {
          type = types.nullOr types.str;
          default = "immich";
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
    description = lib.mdDoc "Immich service configuration";
  };
}
