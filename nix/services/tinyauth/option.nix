# Tinyauth forward authentication service options.
{...}: {
  flake.modules.nixos.tinyauth-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.tinyauth = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "tinyauth forward authentication service";
              port = mkOption {
                type = types.port;
                default = 3000;
                description = "Port on which tinyauth listens";
              };
              users = mkOption {
                type = types.listOf types.str;
                default = [];
                description = ''
                  List of users in username:bcrypt_hash format.
                  Generate with: docker run -i -t --rm ghcr.io/steveiliop56/tinyauth:v5 user create --interactive
                '';
              };
              sessionExpiry = mkOption {
                type = types.int;
                default = 86400;
                description = "Session expiry time in seconds (default 24h)";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "tinyauth";
              auth.enabled = false;
            }
            // lib.neo.mkServiceMeta {
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
            };
        };
        default = {};
        description = "Tinyauth forward authentication service configuration";
      };
    };
}
