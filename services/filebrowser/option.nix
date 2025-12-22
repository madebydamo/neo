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
        domain = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc "Primary domain for swag";
        };
        email = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc "LetsEncrypt email for swag";
        };
        extraDomains = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = lib.mdDoc "Extra domains for swag";
        };
      };
    };
    default = { };
    description = lib.mdDoc "Filebrowser service configuration";
  };
}