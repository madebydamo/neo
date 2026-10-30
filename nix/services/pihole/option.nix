# Pi-hole service options.
{...}: {
  flake.modules.nixos.pihole-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.pihole = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Pi-hole, which blocks ads and trackers for every device on your home network" {rank = 0;};
              # FTLCONF_dns_upstreams.
              upstream = mkOption {
                type = types.str;
                default = "9.9.9.9;1.1.1.1";
                description = "Internet DNS servers Pi-hole asks after filtering, separated by semicolons. Default: Quad9 (9.9.9.9) and Cloudflare (1.1.1.1).";
                rank = 10;
                ui = lib.neo.ui.mkUi {
                  label = "Upstream DNS servers";
                  group = lib.neo.ui.groups.advanced;
                };
              };
              # Answers every local DNS name (services.swag.domain and subdomains) with this
              # address via FTLCONF_misc_dnsmasq_lines. Required with Tailscale split DNS, so
              # Pi-hole binds only this address and leaves the Tailscale IP for dnsmasq.
              localIP = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Your homeserver's address on your home network, e.g. 192.168.1.10. Recommended: devices at home then reach your apps directly instead of going out over the internet.";
                rank = 20;
                ui = lib.neo.ui.mkUi {label = "Home network address";};
              };
              # FTLCONF_webserver_api_password.
              webPassword = lib.neo.mkSecretOption {
                type = types.str;
                default = "";
                description = "Password for the Pi-hole admin page. Empty means no Pi-hole password. Always set one if you turn off the homeserver login for Pi-hole.";
                rank = 30;
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "Admin password";};
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "pihole";
              auth.enabled = true;
            }
            // lib.neo.mkContainerDefinitions {
              pihole = "pihole/pihole:latest";
              timerUnits = ["pihole-update-gravity"];
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/pihole"
            // lib.neo.mkServiceMeta {
              category = "Network";
              icon = ./icon.svg;
              tagline = "Blocks ads for every device at home";
              description = ''
                Removes ads and trackers on phones, TVs, laptops and apps without installing anything on them.
                A dashboard shows which devices contact which sites, and you can block or allow sites yourself.
              '';
              notes = ''
                Works as the DNS server of your network: set this server's address as DNS in your router. Blocking happens at DNS level, so ads are never downloaded.
                Supports regex blocklists and can act as DHCP server.
                When the server is off, devices that use it as DNS cannot open websites: switch the router's DNS to a public server such as 9.9.9.9 until it is back.
              '';
              privacy = ["browsing"];
              projectUrl = "https://pi-hole.net/";
              githubUrl = "https://github.com/pi-hole/pi-hole";
              releaseUrl = "https://github.com/pi-hole/pi-hole/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Pi-hole service configuration";
      };
    };
}
