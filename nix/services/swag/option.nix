# SWAG reverse proxy service options.
{...}: {
  flake.modules.nixos.swag-option = {lib, ...}:
    with lib; {
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
              default = [];
              description = lib.mdDoc "Extra domains for swag";
            };
            onlySubdomains = mkOption {
              type = types.bool;
              default = true;
              description = lib.mdDoc "Only use subdomains (ONLY_SUBDOMAINS)";
            };
            additionalMountPoints = mkOption {
              type = types.attrsOf types.str;
              default = {};
              description = lib.mdDoc "Additional volume mounts";
            };
            localHttpPort = mkOption {
              type = types.port;
              internal = true;
              default = 80;
              description = lib.mdDoc "Local HTTP port for SWAG container (overridden to 9980 with streamproxy)";
            };
            localHttpsPort = mkOption {
              type = types.port;
              internal = true;
              default = 443;
              description = lib.mdDoc "Local HTTPS port for SWAG container (overridden to 9981 with streamproxy)";
            };
          };
        };
        default = {};
        description = lib.mdDoc "Swag service configuration";
      };
    };
}
