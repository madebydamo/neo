# Shared reverse-proxy option types for services (subdomain, auth, custom domains).
# Ranks are level-dependent — see nix/lib/option.nix for the service band table.
{lib, ...}: let
  neoUi = (import ../ui.nix {inherit lib;}).libExtensions.ui.neo.ui;
  inherit (neoUi.groups) access advanced;
in {
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
              description = "First part of this app's web address, e.g. photos gives photos.<your domain>. Lowercase letters only, and each app needs its own.";
            }
            // {
              rank = 100;
              ui = neoUi.mkUi {
                group = access;
                label = "Subdomain";
                summary = true;
              };
            };

          ingress =
            mkOption {
              type = types.listOf (types.enum ["local" "tailscale" "web"]);
              default = ["local" "tailscale" "web"];
              # Internet = public HTTPS via the rathole/streamproxy listener; unticked
              # places get the neo 404 page. Let's Encrypt HTTP-01 on port 80 is always allowed.
              description = "Where this app can be opened from. Places not ticked are blocked. Home network means devices at home, Tailscale your own devices anywhere, Internet everyone.";
            }
            // {
              rank = 105;
              ui = neoUi.mkUi {
                group = access;
                label = "Reachable from";
                summary = true;
                choices = ["local" "tailscale" "web"];
                choiceLabels = {
                  local = "Home network";
                  tailscale = "Tailscale";
                  web = "Internet";
                };
              };
            };

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
              # Added to SWAG (certificates) and Pi-hole (local DNS).
              description = "Extra domain names you own that should open this app (e.g. www.example.com). HTTPS is set up automatically.";
            }
            // {
              rank = 130;
              ui = neoUi.mkUi {
                group = access;
                label = "Custom domains";
              };
            };

          auth =
            mkOption {
              type = types.submodule {
                options = {
                  enabled =
                    mkOption {
                      type = types.bool;
                      default = auth.enabled;
                      description = "Ask everyone to log in before they can open this app.";
                    }
                    // {
                      rank = 0;
                      ui = neoUi.mkUi {
                        label = "Require login";
                        summary = true;
                      };
                    };
                  publicPaths =
                    mkOption {
                      type = types.listOf types.str;
                      default = auth.publicPaths;
                      # Regexes matched against the request path; matches bypass tinyauth.
                      description = "Web addresses (patterns) that open without login, e.g. share links or phone app APIs.";
                    }
                    // {
                      rank = 10;
                      ui = neoUi.mkUi {
                        group = advanced;
                        label = "Public paths";
                        visibleWhen = "enabled";
                      };
                    };
                };
              };
              default = auth;
              internal = !authAvailable;
              # Tinyauth forward authentication.
              description = "Login protection for this app.";
            }
            // {
              rank = 120;
              ui = neoUi.mkUi {group = access;};
            };
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
