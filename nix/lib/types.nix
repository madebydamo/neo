# Shared option types for services.
{lib, ...}: {
  libExtensions.types = {
    neo = {
      mkReverseProxyOptions = {
        subdomain ? null,
        auth ? null,
      } @ args: let
        defaultAuth = {
          enabled = true;
          publicPaths = [];
        };
        auth = lib.recursiveUpdate defaultAuth (args.auth or {});
      in
        with lib; {
          subdomain = mkOption {
            type = types.nullOr types.str;
            default = subdomain;
            description = lib.mdDoc "Subdomain for the service (used by swag reverse proxy)";
          };
          proxyConf = mkOption {
            type = types.nullOr types.str;
            default = null;
            description = lib.mdDoc "Nginx proxy conf for swag";
          };
          auth = mkOption {
            type = types.submodule {
              options = {
                enabled = mkEnableOption (lib.mdDoc "tinyauth forward auth");
                publicPaths = mkOption {
                  type = types.listOf types.str;
                  default = auth.publicPaths;
                  description = lib.mdDoc "Regex paths that bypass tinyauth authentication";
                };
              };
            };
            default = auth;
            description = lib.mdDoc "Tinyauth forward authentication settings";
          };
        };
    };
  };
}
