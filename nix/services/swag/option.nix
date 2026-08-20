# SWAG reverse proxy service options.
{...}: {
  flake.modules.nixos.swag-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.swag = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "swag service" {rank = 0;};
              domain = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Primary domain for swag";
                rank = 10;
              };
              email = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "LetsEncrypt email for swag";
                rank = 20;
              };
              proxyPass = mkOption {
                type = types.attrsOf types.str;
                default = {};
                description = "Map of extra domains to http upstream URLs (plain http backends) to create direct proxy server blocks for (e.g. { \"octo.example.com\" = \"http://192.168.178.42:8123\"; }). SWAG handles TLS termination; no need if the target already speaks HTTPS.";
                rank = 30;
              };
              onlySubdomains = mkOption {
                type = types.bool;
                default = true;
                description = "Only use subdomains";
                rank = 40;
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
                description = "Host port mapped to container 443 (plain TLS, LAN). Overridden to 9981 with streamproxy.";
              };
              localHttpsProxyProtocolPort = mkOption {
                type = types.port;
                internal = true;
                default = 8443;
                description = "Host port mapped to container 8443 (TLS + PROXY protocol for streamproxy/rathole). Overridden to 9982 with streamproxy.";
              };
              geo = mkOption {
                type = types.submodule {
                  options = {
                    countryWhitelist = mkOption {
                      type = types.listOf types.str;
                      default = [];
                      description = ''
                        ISO 3166-1 alpha-2 country codes allowed to reach proxied services (e.g. ["CH" "DE"]).
                        Empty (default): all countries allowed. When non-empty, only listed countries (plus LAN) are allowed.
                      '';
                      rank = 0;
                      example = ["CH" "DE" "AT"];
                    };
                    countryBlacklist = mkOption {
                      type = types.listOf types.str;
                      default = [];
                      description = ''
                        ISO 3166-1 alpha-2 country codes blocked at the edge (e.g. ["RU" "CN"]).
                        Empty (default): no countries blocked. Combined with whitelist: a request must pass both.
                      '';
                      rank = 10;
                      example = ["RU" "CN" "KP"];
                    };
                    continentBlacklist = mkOption {
                      type = types.listOf types.str;
                      default = [];
                      description = ''
                        Continent codes to block: AF, AS, EU, NA, OC, SA, AN.
                        Empty (default): no continents blocked.
                      '';
                      rank = 20;
                      example = ["AS" "AF"];
                    };
                  };
                };
                default = {};
                description = "GeoIP allow/deny lists (DB-IP). Empty lists leave access unrestricted; LAN ranges always bypass.";
                rank = 50;
              };
            }
            # Dashboard UI at https://swag.<domain>/ (docker-mod + GoAccess).
            // lib.neo.mkReverseProxyOptions {
              subdomain = "swag";
            }
            // lib.neo.mkContainerDefinitions {
              swag = "lscr.io/linuxserver/swag:latest";
              extraUnits = [
                "swag-cert-reloader"
                "swag-patcher"
              ];
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/swag"
            // lib.neo.mkServiceMeta {
              category = "Core";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/nginx.svg";
              description = ''
                SWAG (Secure Web Application Gateway) is the foundational reverse proxy and SSL termination layer for the Neo homeserver.
                Built on Nginx with integrated Certbot, it automatically provisions and renews trusted SSL certificates from Let's Encrypt or ZeroSSL for the primary domain, all configured subdomains, and domains listed in proxyPass.
                Every other service's public web interface is routed exclusively through SWAG using its extensive library of reverse proxy configurations, enabling centralized HTTPS, optional auth, fail2ban protection, and keeping backends isolated on the internal Docker network.
                When enabled, SWAG also loads the linuxserver swag-dashboard and swag-dbip docker-mods (GoAccess analytics + DB-IP GeoIP for the geographic overview). Optional country/continent allow and deny lists live under services.swag.geo (empty = unrestricted).
                As the entry point for all external traffic, SWAG must be configured with your domain and email before other proxied services can be reached securely from the internet. It uses the container image lscr.io/linuxserver/swag:latest.
              '';
              projectUrl = "https://docs.linuxserver.io/general/swag/";
              githubUrl = "https://github.com/linuxserver/docker-swag";
              releaseUrl = "https://github.com/linuxserver/docker-swag/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Swag service configuration";
      };
    };
}
