# SWAG reverse proxy service options.
{...}: {
  flake.modules.nixos.swag-option = {lib, ...}:
    with lib; {
      options.neo.services.swag = mkOption {
        type = types.submodule {
          options = {
            enabled = mkEnableOption "swag service";
            domain = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Primary domain for swag";
            };
            email = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "LetsEncrypt email for swag";
            };
            extraDomains = mkOption {
              type = types.listOf types.str;
              default = [];
              description = "Extra domains for swag";
            };
            onlySubdomains = mkOption {
              type = types.bool;
              default = true;
              description = "Only use subdomains (ONLY_SUBDOMAINS)";
            };
            localHttpPort = mkOption {
              type = types.port;
              internal = true;
              default = 80;
              description = "Local HTTP port for SWAG container (overridden to 9980 with streamproxy)";
            };
            localHttpsPort = mkOption {
              type = types.port;
              internal = true;
              default = 443;
              description = "Local HTTPS port for SWAG container (overridden to 9981 with streamproxy)";
            };
          };
        };
        default = {};
        description = "Swag service configuration";
      };
    };
}
