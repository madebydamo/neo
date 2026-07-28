# Shared reverse-proxy option types for services (subdomain, auth, custom domains).
# Ranks are level-dependent — see nix/lib/option.nix for the service band table.
{lib, ...}: {
  libExtensions.reverseProxy = {
    neo = {
      mkReverseProxyOptions = {
        subdomain ? null,
        auth ? null,
        customDomains ? [],
      } @ args: let
        authAvailable = args.auth.available or true;
        defaultAuth = {
          enabled = authAvailable;
          publicPaths = [];
        };
        auth = lib.recursiveUpdate defaultAuth (builtins.removeAttrs (args.auth or {}) ["available"]);
      in
        with lib; {
          subdomain =
            mkOption {
              type = types.nullOr types.str;
              default = subdomain;
              description = "Subdomain for the service (used by swag reverse proxy)";
            }
            // {rank = 100;};

          proxyConf = mkOption {
            type = types.nullOr types.str;
            internal = true;
            default = null;
            description = "Nginx proxy conf for swag";
          };

          customDomains =
            mkOption {
              type = types.listOf types.str;
              default = customDomains;
              description = "Custom domains (one domain per string, e.g. example.com or www.example.com) that should resolve to this service; automatically added to SWAG for certificates and to Pi-hole for local DNS";
            }
            // {rank = 130;};

          auth =
            mkOption {
              type = types.submodule {
                options = {
                  enabled =
                    mkOption {
                      type = types.bool;
                      default = auth.enabled;
                      description = "tinyauth forward auth";
                    }
                    // {rank = 0;};
                  publicPaths =
                    mkOption {
                      type = types.listOf types.str;
                      default = auth.publicPaths;
                      description = "Regex paths that bypass tinyauth authentication";
                    }
                    // {rank = 10;};
                };
              };
              default = auth;
              internal = !authAvailable;
              description = "Tinyauth forward authentication settings";
            }
            // {rank = 120;};
        };
      # Enabled services that get a SWAG subdomain (certs, DNS, proxy-conf materialization).
      # Includes swag itself for the dashboard UI at swag.<domain>.
      # Do NOT require proxyConf here — reading it while modules define it causes infinite recursion.
      getProxiedServices = config:
        lib.filterAttrs (
          _: v: v.enabled && (v.subdomain or null) != null
        )
        config.neo.services;
    };
  };
}
