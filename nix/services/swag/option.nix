# SWAG reverse proxy service options.
{...}: {
  flake.modules.nixos.swag-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      # Single IPv4/IPv6 address or CIDR range (nginx geo syntax).
      ipRange = types.strMatching "([0-9]{1,3}(\\.[0-9]{1,3}){3}|[0-9a-fA-F:]*:[0-9a-fA-F:.]*)(/[0-9]{1,3})?";
      geoGroup = lib.neo.ui.mkGroup {
        id = "geo";
        label = "Geo & IP filter";
        description = "Allow or block visitors by country, continent or IP range";
        icon = lib.neo.ui.icons.map;
        rank = 110;
      };
      autoBanGroup = lib.neo.ui.mkGroup {
        id = "autoban";
        label = "Auto ban";
        description = "Fail2ban: temporarily ban IPs that trip a jail (failed logins, bad bots, exploit scans)";
        icon = lib.neo.ui.icons.shield;
        rank = 120;
      };
      # SWAG's stock fail2ban jails (filters ship in the image).
      jailLabels = {
        nginx-http-auth = "Failed basic auth";
        nginx-unauthorized = "Repeated 401 responses";
        nginx-badbots = "Bad bot user agents";
        nginx-botsearch = "Exploit path scans";
        nginx-deny = "Nginx deny hits";
      };
      jailNames = attrNames jailLabels;
    in {
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
                ui = lib.neo.ui.mkUi {
                  label = "Extra domain routes";
                  widget = "proxyRouteList";
                  entryLabel = "Route";
                };
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
                    ipWhitelist = mkOption {
                      type = types.listOf ipRange;
                      default = [];
                      description = ''
                        IP addresses or CIDR ranges (IPv4/IPv6) always allowed, even from a blocked country or continent
                        or outside the country whitelist (e.g. a WARP / mobile egress range).
                        IP rules take precedence over country rules.
                      '';
                      rank = 30;
                      example = ["104.28.253.0/24" "203.0.113.7"];
                    };
                    ipBlacklist = mkOption {
                      type = types.listOf ipRange;
                      default = [];
                      description = ''
                        IP addresses or CIDR ranges (IPv4/IPv6) always blocked, even from a whitelisted country or the LAN.
                        IP rules take precedence over country rules; when whitelist and blacklist overlap, the more specific range wins.
                      '';
                      rank = 40;
                      example = ["198.51.100.0/24"];
                    };
                  };
                };
                default = {};
                description = "GeoIP and IP allow/deny lists (DB-IP). Empty lists leave access unrestricted. IP rules beat country/continent rules; LAN bypasses country rules but not the IP blacklist.";
                rank = 50;
                ui = lib.neo.ui.mkUi {group = geoGroup;};
              };
              autoBan = mkOption {
                type = types.submodule {
                  options = {
                    jails = mkOption {
                      type = types.listOf (types.enum jailNames);
                      default = jailNames;
                      description = ''
                        Fail2ban jails to run. nginx-http-auth bans failed basic-auth logins, nginx-unauthorized repeated 401s,
                        nginx-badbots known bad user agents, nginx-botsearch scans for common exploit paths and nginx-deny
                        requests nginx denied. Empty list: fail2ban does not run at all.
                      '';
                      rank = 0;
                      ui = lib.neo.ui.mkUi {
                        label = "Active jails";
                        summary = true;
                        choices = jailNames;
                        choiceLabels = jailLabels;
                      };
                    };
                    ignoreIps = mkOption {
                      type = types.listOf ipRange;
                      default = [];
                      description = ''
                        IP addresses or CIDR ranges (IPv4/IPv6) fail2ban never bans, e.g. your own public IPs.
                        LAN ranges (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16) are always ignored.
                        Not the same as geo.ipWhitelist, which only bypasses the geo filter.
                      '';
                      rank = 10;
                      example = ["203.0.113.7" "198.51.100.0/24"];
                      ui = lib.neo.ui.mkUi {label = "Never ban";};
                    };
                    banTime = mkOption {
                      type = types.ints.positive;
                      default = 600;
                      description = "Seconds a banned IP stays banned.";
                      rank = 20;
                      ui = lib.neo.ui.mkUi {label = "Ban time (s)";};
                    };
                    findTime = mkOption {
                      type = types.ints.positive;
                      default = 600;
                      description = "Window in seconds in which maxRetry failures trigger a ban.";
                      rank = 30;
                      ui = lib.neo.ui.mkUi {label = "Find window (s)";};
                    };
                    maxRetry = mkOption {
                      type = types.ints.positive;
                      default = 5;
                      description = "Failures within findTime before an IP is banned (nginx-badbots always bans after 2).";
                      rank = 40;
                      ui = lib.neo.ui.mkUi {label = "Max retries";};
                    };
                  };
                };
                default = {};
                description = "Fail2ban inside SWAG: bans IPs that trip the selected jails. Disabling every jail stops fail2ban entirely.";
                rank = 60;
                ui = lib.neo.ui.mkUi {group = autoBanGroup;};
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
              icon = ./icon.svg;
              description = ''
                SWAG (Secure Web Application Gateway) is the foundational reverse proxy and SSL termination layer for the Neo homeserver.
                Built on Nginx with integrated Certbot, it automatically provisions and renews trusted SSL certificates from Let's Encrypt or ZeroSSL for the primary domain, all configured subdomains, and domains listed in proxyPass.
                Every other service's public web interface is routed exclusively through SWAG using its extensive library of reverse proxy configurations, enabling centralized HTTPS, optional auth, fail2ban protection, and keeping backends isolated on the internal Docker network.
                When enabled, SWAG also loads the linuxserver swag-dashboard and swag-dbip docker-mods (GoAccess analytics + DB-IP GeoIP for the geographic overview). Optional country/continent and IP/CIDR allow and deny lists live under services.swag.geo (empty = unrestricted; IP rules take precedence over country rules); fail2ban jails and never-ban IPs under services.swag.autoBan (no jails = fail2ban off).
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
