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
            description = "Subdomain for the service (used by swag reverse proxy)";
          };
          proxyConf = mkOption {
            type = types.nullOr types.str;
            default = null;
            description = "Nginx proxy conf for swag";
          };
          auth = mkOption {
            type = types.submodule {
              options = {
                enabled = mkEnableOption ("tinyauth forward auth");
                publicPaths = mkOption {
                  type = types.listOf types.str;
                  default = auth.publicPaths;
                  description = "Regex paths that bypass tinyauth authentication";
                };
              };
            };
            default = auth;
            description = "Tinyauth forward authentication settings";
          };
        };
    };
  };
}
