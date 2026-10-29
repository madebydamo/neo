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
              enabled = mkEnableOption "Nextcloud, your own cloud for files, sharing and collaboration" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 80;
                description = "Internal port for nextcloud apache web server";
              };
              # MYSQL_PASSWORD of the nextcloud user; also MYSQL_ROOT_PASSWORD of the db container.
              dbPassword = lib.neo.mkSecretOption {
                type = types.str;
                default = "your_strong_password_here";
                rank = 10;
                description = "Internal database password; you never type it. Press Generate before the first start. Changing it after setup breaks Nextcloud.";
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {
                  label = "Database password";
                  group = lib.neo.ui.groups.advanced;
                };
              };
              # occ maintenance_window_start (Nextcloud reads it as UTC).
              maintenanceWindowStart = mkOption {
                type = types.ints.between 0 23;
                default = 3;
                rank = 20;
                description = "Hour of the night (0–23, UTC) when Nextcloud runs heavy cleanup tasks. Default 3 = 3 AM UTC.";
                ui = lib.neo.ui.mkUi {label = "Nightly maintenance hour";};
              };
              # occ default_phone_region (ISO 3166-1 alpha-2).
              defaultPhoneRegion = mkOption {
                type = types.str;
                default = "CH";
                rank = 30;
                description = "Your country as a two-letter code (e.g. CH, US, DE), so phone numbers without a + prefix are read correctly.";
                ui = lib.neo.ui.mkUi {label = "Phone number country";};
              };
              # occ instanceid. Nextcloud names its appdata_<id> folder after it.
              instanceId = mkOption {
                type = types.str;
                default = "neo-homeserver";
                rank = 40;
                description = "Internal name of this Nextcloud install. Leave as is.";
                ui = lib.neo.ui.mkUi {
                  label = "Instance name";
                  group = lib.neo.ui.groups.advanced;
                };
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
              icon = ./icon.svg;
              tagline = "Your own cloud for files, calendars and contacts";
              description = ''
                Sync your files across computers and phones, share them with a link, and keep calendars and contacts in one place, like Google Drive or iCloud.
                Turn on Collabora to edit documents together in the browser.
              '';
              notes = ''
                Also offers Talk (video calls and chat), groupware and an AI assistant as Nextcloud apps.
              '';
              replaces = ["Google Drive" "Dropbox" "iCloud Drive"];
              privacy = ["files" "calendar" "contacts"];
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
