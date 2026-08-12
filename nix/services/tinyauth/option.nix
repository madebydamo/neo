# Tinyauth forward authentication service options.
{...}: {
  flake.modules.nixos.tinyauth-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.tinyauth = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "tinyauth forward authentication service" {rank = 0;};
              users = mkOption {
                type = types.listOf types.str;
                default = [];
                description = ''
                  List of users in username:bcrypt_hash format.
                  Use "Add user" in the web UI, or generate with:
                  docker run -i -t --rm ghcr.io/steveiliop56/tinyauth:v5 user create --interactive
                '';
                rank = 10;
                helper = lib.neo.helpers.bcryptUser;
              };
              # Per-user app ACLs. Keys = usernames (before ":" in users).
              # allow/block list neo service names.
              access = mkOption {
                type = types.attrsOf (types.submodule {
                  options = {
                    allow = mkOption {
                      type = types.listOf types.str;
                      default = [];
                      description = ''
                        Whitelist of neo service names this user may access.
                        If non-empty, the user may only use these apps (do not also set block).
                      '';
                      rank = 0;
                    };
                    block = mkOption {
                      type = types.listOf types.str;
                      default = [];
                      description = ''
                        Blacklist of neo service names this user may not access.
                        Only applied when allow is empty.
                      '';
                      rank = 10;
                    };
                  };
                });
                default = {};
                description = ''
                  Per-user application access control. Attribute names are Tinyauth usernames
                  (the part before ":" in users). Empty allow and block means full access.
                  Orphan keys for deleted users are ignored at evaluation and dropped on save
                  when the UI only shows active users.
                '';
                rank = 15;
              };

              sessionExpiry = mkOption {
                type = types.int;
                default = 86400;
                description = "Session expiry time in seconds (default 24h)";
                rank = 20;
              };
              port = mkOption {
                type = types.port;
                default = 3000;
                internal = true;
                description = "Port on which tinyauth listens";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "tinyauth";
              auth.available = false;
            }
            // lib.neo.mkContainerDefinitions {
              tinyauth = "ghcr.io/steveiliop56/tinyauth:v5";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/tinyauth"
            // lib.neo.mkServiceMeta {
              category = "Security";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/tinyauth.svg";
              description = ''
                Tinyauth is the tiniest OpenID Connect (OIDC) authentication and authorization server for your self-hosted applications.
                It functions as lightweight middleware adding secure username/password (bcrypt + optional TOTP), OAuth (GitHub, Google, generic), and LDAP login flows, working seamlessly with reverse proxies such as SWAG/Nginx, Traefik, and Caddy.
                Configuration happens exclusively through environment variables — no dashboards, databases, or complex setup files needed — keeping the statically linked binary tiny and extremely low on resources.
                In the Neo homeserver it acts as the central forward-auth provider, protecting the web UIs of other services while providing its own clean login page.
              '';
              projectUrl = "https://tinyauth.app/";
              githubUrl = "https://github.com/tinyauthapp/tinyauth";
              releaseUrl = "https://github.com/tinyauthapp/tinyauth/releases";
              rank = 10;
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Tinyauth forward authentication service configuration";
      };
    };
}
