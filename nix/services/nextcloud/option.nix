# Nextcloud service options. Web UI protected with tinyauth forward auth.
{...}: {
  flake.modules.nixos.nextcloud-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.nextcloud = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "nextcloud file sharing and collaboration platform" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 80;
                description = "Internal port for nextcloud apache web server";
              };
              dbPassword = mkOption {
                type = types.str;
                default = "your_strong_password_here";
                rank = 10;
                description = "Password for the nextcloud mysql user (also used for root in db container)";
                helper = lib.neo.helpers.randomToken;
              };
              maintenanceWindowStart = mkOption {
                type = types.ints.between 0 23;
                default = 3;
                rank = 20;
                description = "Hour (0-23) when maintenance window starts (for background jobs)";
              };
              defaultPhoneRegion = mkOption {
                type = types.str;
                default = "CH";
                rank = 30;
                description = "ISO 3166-1 alpha-2 country code for default phone region (e.g. CH, US)";
              };
              instanceId = mkOption {
                type = types.str;
                default = "neo-homeserver";
                rank = 40;
                description = "Unique server/instance identifier (used by Nextcloud for clustering/multi-server setups)";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "nextcloud";
              auth.publicPaths = [
                "^/.well-known/"
                "^/remote.php/"

                # === Collabora / Nextcloud Office (required for editor to work) ===
                "^/apps/richdocuments/"
                "^/index.php/apps/richdocuments/"
                "^/hosting/(discovery|capabilities)"
                "^/(browser|cool|lool)/"
                "^/index.php/apps/richdocuments/wopi/"
                "^/apps/richdocuments/wopi/"

                # === Other essential public paths ===
                "^/login"
                "^/logout"
                "^/status.php"
                "^/cron.php"
                "^/ocs/v2.php/"
              ];
            }
            // lib.neo.mkContainerDefinitions {
              "nextcloud-db" = "mariadb:10.6";
              "nextcloud-redis" = "redis:alpine";
              "nextcloud" = "nextcloud:apache";
              "nextcloud-cron" = "nextcloud:apache";
              extraUnits = ["nextcloud-setup"];
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/nextcloud"
            // lib.neo.mkServiceMeta {
              category = "Files";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/nextcloud.svg";
              description = ''
                Nextcloud is an open source, self-hosted content collaboration platform that gives you a safe home for all your data.
                Access, sync, and share your files, calendars, contacts, mail, and more from web, desktop, and mobile clients on your own terms.
                It features Nextcloud Office for collaborative document editing, Talk for private video calls and chat, Groupware tools, and an integrated AI assistant — all without relying on third-party clouds.
              '';
              projectUrl = "https://nextcloud.com/";
              githubUrl = "https://github.com/nextcloud/server";
              releaseUrl = "https://nextcloud.com/changelog/";
              iframeCompatible = false;
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Nextcloud service configuration";
      };
    };
}
