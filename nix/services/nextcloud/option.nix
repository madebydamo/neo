# Nextcloud service options. Web UI protected with tinyauth forward auth.
{...}: {
  flake.modules.nixos.nextcloud-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.nextcloud = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption (mdDoc "nextcloud file sharing and collaboration platform");
              port = mkOption {
                type = types.port;
                default = 80;
                description = mdDoc "Internal port for nextcloud apache web server";
              };
              dbPassword = mkOption {
                type = types.str;
                default = "your_strong_password_here";
                description = mdDoc "Password for the nextcloud mysql user (also used for root in db container)";
              };
              maintenanceWindowStart = mkOption {
                type = types.ints.between 0 23;
                default = 3;
                description = mdDoc "Hour (0-23) when maintenance window starts (for background jobs)";
              };
              defaultPhoneRegion = mkOption {
                type = types.str;
                default = "CH";
                description = mdDoc "ISO 3166-1 alpha-2 country code for default phone region (e.g. CH, US)";
              };
              instanceId = mkOption {
                type = types.str;
                default = "neo-homeserver";
                description = mdDoc "Unique server/instance identifier (used by Nextcloud for clustering/multi-server setups)";
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
            };
        };
        default = {};
        description = mdDoc "Nextcloud service configuration";
      };
      options.neo.services.collabora = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption (mdDoc "Collarbora real time collaboration platform");
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "collabora";
              auth.enabled = false;
            };
        };
        default = {};
        description = mdDoc "Nextcloud service configuration";
      };
    };
}
