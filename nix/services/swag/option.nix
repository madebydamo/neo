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
              enabled = mkEnableOption "the web gateway that gives every app a secure HTTPS address" {rank = 0;};
              domain = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Your domain name (e.g. example.com). Every app gets an address under it.";
                rank = 10;
                ui = lib.neo.ui.mkUi {label = "Your domain";};
              };
              email = mkOption {
                type = types.nullOr types.str;
                default = null;
                # Let's Encrypt account email (EMAIL)
                description = "Email address for warnings about your HTTPS certificates.";
                rank = 20;
                ui = lib.neo.ui.mkUi {label = "Certificate email";};
              };
              proxyPass = mkOption {
                type = types.attrsOf types.str;
                default = {};
                # Direct proxy server blocks; SWAG terminates TLS for plain-http backends.
                description = "Give other devices or apps a secure HTTPS address, e.g. octo.example.com for http://192.168.178.42:8123. Not needed if they already use HTTPS.";
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
                # ONLY_SUBDOMAINS
                description = "Get certificates only for app subdomains, not for the bare domain itself.";
                rank = 40;
                ui = lib.neo.ui.mkUi {label = "Subdomains only";};
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
                      # ISO 3166-1 alpha-2 codes
                      description = "Two-letter country codes allowed to reach your apps, e.g. CH, DE. Empty allows all countries; otherwise only these (plus your home network).";
                      ui = lib.neo.ui.mkUi {label = "Allowed countries";};
                      rank = 0;
                      example = ["CH" "DE" "AT"];
                    };
                    countryBlacklist = mkOption {
                      type = types.listOf types.str;
                      default = [];
                      # ISO 3166-1 alpha-2 codes
                      description = "Two-letter country codes that are blocked, e.g. RU, CN. Empty blocks none. A visitor must pass both this and Allowed countries.";
                      ui = lib.neo.ui.mkUi {label = "Blocked countries";};
                      rank = 10;
                      example = ["RU" "CN" "KP"];
                    };
                    continentBlacklist = mkOption {
                      type = types.listOf types.str;
                      default = [];
                      description = "Continents that are blocked: AF, AS, EU, NA, OC, SA or AN. Empty blocks none.";
                      ui = lib.neo.ui.mkUi {label = "Blocked continents";};
                      rank = 20;
                      example = ["AS" "AF"];
                    };
                    ipWhitelist = mkOption {
                      type = types.listOf ipRange;
                      default = [];
                      description = "IP addresses or ranges (e.g. 203.0.113.0/24) that always get through, even from a blocked country. Useful for your phone's mobile network.";
                      ui = lib.neo.ui.mkUi {label = "Always allow IPs";};
                      rank = 30;
                      example = ["104.28.253.0/24" "203.0.113.7"];
                    };
                    ipBlacklist = mkOption {
                      type = types.listOf ipRange;
                      default = [];
                      # Overlap with ipWhitelist: the more specific range wins.
                      description = "IP addresses or ranges that are always blocked, even from an allowed country or your home network.";
                      ui = lib.neo.ui.mkUi {label = "Always block IPs";};
                      rank = 40;
                      example = ["198.51.100.0/24"];
                    };
                  };
                };
                default = {};
                # GeoIP data from DB-IP. LAN bypasses country rules but not ipBlacklist.
                description = "Allow or block visitors by country, continent or IP address. Empty lists let everyone in; IP rules win over country rules.";
                rank = 50;
                ui = lib.neo.ui.mkUi {group = geoGroup;};
              };
              autoBan = mkOption {
                type = types.submodule {
                  options = {
                    jails = mkOption {
                      type = types.listOf (types.enum jailNames);
                      default = jailNames;
                      # Stock SWAG fail2ban jails; an empty list stops fail2ban entirely.
                      description = "Kinds of misbehaviour that get a visitor banned. Select none to turn auto ban off.";
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
                      # LAN ranges 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16 are always ignored.
                      description = "IP addresses or ranges that are never banned, e.g. your own public IP. Your home network is never banned. Always allow IPs only skips the country filter, not this.";
                      rank = 10;
                      example = ["203.0.113.7" "198.51.100.0/24"];
                      ui = lib.neo.ui.mkUi {label = "Never ban";};
                    };
                    banTime = mkOption {
                      type = types.ints.positive;
                      default = 600;
                      description = "How many seconds a banned visitor stays blocked.";
                      rank = 20;
                      ui = lib.neo.ui.mkUi {label = "Ban time (s)";};
                    };
                    findTime = mkOption {
                      type = types.ints.positive;
                      default = 600;
                      description = "Time window in seconds: reaching Max retries failures within it gets a visitor banned.";
                      rank = 30;
                      ui = lib.neo.ui.mkUi {label = "Find window (s)";};
                    };
                    maxRetry = mkOption {
                      type = types.ints.positive;
                      default = 5;
                      # nginx-badbots always bans after 2.
                      description = "How many failures within the Find window lead to a ban.";
                      rank = 40;
                      ui = lib.neo.ui.mkUi {label = "Max retries";};
                    };
                  };
                };
                default = {};
                # fail2ban inside the SWAG container
                description = "Temporarily ban visitors who fail to log in repeatedly or scan for weaknesses.";
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
              tagline = "Secure web addresses (HTTPS) for all your apps";
              description = ''
                The front door of your homeserver: every app gets its own address like photos.example.com, with free HTTPS certificates that renew automatically, and visitors who misbehave are blocked.
                Set your domain and email here first.
              '';
              notes = ''
                SWAG (nginx with Certbot) terminates TLS and proxies to the apps on the internal Docker network. Certificates come from Let's Encrypt or ZeroSSL for the domain, all subdomains and the proxyPass domains.
                Loads the swag-dashboard and swag-dbip mods (GoAccess analytics and GeoIP). Country and IP allow/deny lists live under geo (empty = unrestricted; IP rules win over country rules); fail2ban jails and never-ban IPs under autoBan (no jails = fail2ban off).
                Image: lscr.io/linuxserver/swag:latest.
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
