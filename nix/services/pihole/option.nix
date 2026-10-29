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
              enabled = mkEnableOption "pihole ad-blocking DNS service" {rank = 0;};
              upstream = mkOption {
                type = types.str;
                default = "9.9.9.9;1.1.1.1";
                description = "Semicolon separated list of upstream dns servers";
                rank = 10;
              };
              localIP = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Local IP address to forward services.swag.domain toward to. Required when Tailscale split DNS is enabled, so Pi-hole binds this address only and leaves the Tailscale IP for dnsmasq.";
                rank = 20;
              };
              webPassword = lib.neo.mkSecretOption {
                type = types.str;
                default = "";
                description = "Optional Password for Pi-hole web admin interface. Do not leave it blank and turn auth off.";
                rank = 30;
                helper = lib.neo.helpers.randomToken;
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
                With Tailscale split DNS, set localIP so Pi-hole answers only on the home network address.
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
