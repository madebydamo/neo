{ config, lib, ... }:

with lib;

{
  options.neo.services.swag = mkOption {
    type = types.submodule {
      options = {
        enabled = mkEnableOption (lib.mdDoc "swag service");
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
    description = lib.mdDoc "Swag service configuration";
  };
}