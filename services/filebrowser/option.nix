{ config, lib, ... }:

with lib;

{
  options.neo.services.filebrowser = mkOption {
    type = types.submodule {
      options = {
        enabled = mkEnableOption (lib.mdDoc "filebrowser service");
        subdomain = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc "Subdomain for the service";
        };
        proxyConf = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc "Nginx proxy conf for swag";
        };
        additionalMountPoints = mkOption {
          type = types.attrsOf types.str;
          default = { };
          description = lib.mdDoc "Additional volume mounts";
        };
      };
    };
    default = { };
    description = lib.mdDoc "Filebrowser service configuration";
  };
}