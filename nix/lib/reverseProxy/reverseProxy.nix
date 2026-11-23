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

      # Whether a custom domain of a service with login answers with a redirect
      # to <subdomain>.<domain> instead of serving the app: always with login.
      # Outside the tinyauth cookie domain the login cannot work at all. Below
      # it (biel.dev.example.com under example.com) tinyauth only sees the
      # app's own address of the hairpin and sends the browser there after the
      # login, so the custom name never stays. Before the X-Forwarded-Host fix
      # in tinyauth-server.conf the hairpin even got tinyauth's 400 ("conflicting
      # auth module headers", nginx 500; meilen 2026-11-23). Temporary 307 on
      # purpose: old names may come back, and it keeps method and body.
      # domain / customDomain stay as arguments for callers (plugins).
      customDomainRedirects = {
        domain,
        customDomain,
        authEnabled,
      }:
        authEnabled;

      # SWAG site conf (site-confs/<customDomain>.conf) for one custom domain:
      # port 80 goes to HTTPS; HTTPS either redirects to the app's own address
      # (307 keeps method and body, before any auth) or proxies to its vhost.
      mkCustomDomainSiteConf = {
        domain,
        subdomain,
        customDomain,
        redirect,
      }: let
        host = "${subdomain}.${domain}";
        location =
          if redirect
          then ''
            # Login only works on the app's own address: send the browser there.
            location / {
              return 307 https://${host}$request_uri;
            }
          ''
          else ''
            location / {
              proxy_pass https://127.0.0.1:443;

              proxy_set_header Host ${host};
              proxy_set_header X-Real-IP $remote_addr;
              proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
              proxy_set_header X-Forwarded-Proto $scheme;
              proxy_set_header X-Forwarded-Host $host;

              proxy_ssl_server_name on;
              proxy_ssl_name ${host};
              proxy_ssl_verify off;

              proxy_http_version 1.1;
              proxy_set_header Connection "";
            }
          '';
      in
        ''
          server {
            listen 80;
            listen [::]:80;
            server_name ${customDomain};

            return 301 https://$server_name$request_uri;
          }

          server {
            include /config/nginx/listen-https.conf;
            http2 on;
            server_name ${customDomain};

            include /config/nginx/ssl.conf;
            client_max_body_size 0;
            include /config/nginx/geo-access.conf;
        ''
        + lib.concatMapStrings (l: "  ${l}\n") (lib.splitString "\n" (lib.removeSuffix "\n" location))
        + "}\n";

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
              # Added to SWAG (certificates) and Pi-hole (local DNS). With login on,
              # every name gets a 307 to <subdomain>.<domain> (customDomainRedirects):
              # the login only works on the app's own address.
              description = "Extra domain names you own that should open this app (e.g. www.example.com or an old address). HTTPS is set up automatically. With Require login on, they send visitors on to this app's own address, where they log in.";
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
