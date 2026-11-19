# Shared reverse-proxy option types for services (subdomain, auth, custom domains).
# Ranks are level-dependent — see nix/lib/option.nix for the service band table.
{lib, ...}: let
  neoUi = (import ../ui.nix {inherit lib;}).libExtensions.ui.neo.ui;
  inherit (neoUi.groups) access advanced;
  inherit ((import ./ingress.nix {inherit lib;}).libExtensions.ingress.neo) ingressVariants;
  publicHost = config: cfg: let
    domain = config.neo.services.swag.domain or null;
    subdomain = cfg.subdomain or null;
  in
    if domain == null || domain == "" || subdomain == null || subdomain == ""
    then null
    else "${subdomain}.${domain}";
in {
  libExtensions.reverseProxy = {
    neo = {
      # A service's public host name (photos.example.com), from its subdomain and
      # the SWAG domain. Null until both are set. Ignores customDomains.
      inherit publicHost;

      # The same as an https:// URL, or null.
      publicUrl = config: cfg: let
        host = publicHost config cfg;
      in
        if host == null
        then null
        else "https://${host}";

      # ingress / auth.enabled follow Default access (neo.access, set by
      # nix/modules/core/access.nix) unless given here: an app that must not
      # follow passes its own value (`ingress = ["local"]`, `auth.enabled = false`).
      mkReverseProxyOptions = {
        subdomain ? null,
        auth ? null,
        customDomains ? [],
        ingress ? null,
      } @ args: let
        authAvailable = args.auth.available or true;
        # Login follows the default only when the app lets the user choose and
        # does not fix its own value.
        loginInherits = authAvailable && !((args.auth or {}) ? enabled);
        defaultAuth = {
          enabled = authAvailable;
          publicPaths = [];
        };
        auth = lib.recursiveUpdate defaultAuth (builtins.removeAttrs (args.auth or {}) ["available"]);
        inherited = from: label: {
          inherits = {
            path = from;
            inherit label;
            fromModule = true;
          };
        };
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
              type = types.listOf (types.enum ingressVariants);
              default =
                if ingress == null
                then ingressVariants
                else ingress;
              # Internet = public HTTPS via the rathole/streamproxy listener; unticked
              # places get the neo 404 page. Let's Encrypt HTTP-01 on port 80 is always allowed.
              description = "Where this app can be opened from. Places not ticked are blocked. Home network means devices at home, Tailscale and NetBird your own devices anywhere, Internet everyone.";
            }
            // {
              rank = 105;
              ui = neoUi.mkUi {
                group = access;
                label = "Reachable from";
                summary = true;
                choices = ingressVariants;
                choiceLabels = {
                  local = "Home network";
                  tailscale = "Tailscale";
                  netbird = "NetBird";
                  web = "Internet";
                };
              };
            }
            // lib.optionalAttrs (ingress == null) (inherited "access.defaultIngress" "Default access");

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
                    }
                    // lib.optionalAttrs loginInherits (inherited "access.defaultLogin" "Default login");
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
      # Sign in to the app with the login page account (OIDC through tinyauth's
      # OIDC server, neo.services.tinyauth.oidcClients). Follows "Sign in to
      # apps with your login page account" (neo.access.defaultSso) unless set
      # for the app. Declare it as `sso` next to the app's other options; the
      # app registers its OIDC client inside `mkIf cfg.sso`.
      mkSsoOption = {
        description ? "Sign in with your login page account, the same one as for your other apps, instead of a separate account for this app.",
        rank ? 125,
        label ? "Sign in with the login page",
      }:
        lib.mkOption {
          type = lib.types.bool;
          default = false;
          inherit description;
        }
        // {
          inherit rank;
          ui = neoUi.mkUi {
            group = access;
            inherit label;
          };
          inherits = {
            path = "access.defaultSso";
            label = "Default sign-in";
            fromModule = true;
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
