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
                description = "Local IP address to forward services.swag.domain toward to.";
                rank = 20;
              };
              webPassword = mkOption {
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
              extraUnits = ["pihole-update-gravity"];
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/pihole"
            // lib.neo.mkServiceMeta {
              category = "Network";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/pi-hole.svg";
              description = ''
                Pi-hole is a powerful network-wide ad blocker that acts as a DNS sinkhole, protecting all devices on your network from unwanted advertisements, trackers, and malware.
                It requires no client software on individual devices and blocks content even in mobile apps, smart TVs, and other non-browser locations.
                By filtering at the DNS level before downloads occur, Pi-hole speeds up browsing, reduces bandwidth usage, and provides detailed statistics, query logs, and an intuitive web dashboard for managing lists and settings.
                It supports regex blocking, can function as a DHCP server, and offers easy integration with VPNs for protection on the go.
                Completely free and open source, Pi-hole puts you in control of your network's privacy and performance.
              '';
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
