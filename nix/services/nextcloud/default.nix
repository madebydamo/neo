# Nextcloud service implementation (db, redis, cron, app, collabora). Web UI is behind tinyauth via swag proxy.
{...}: {
  flake.modules.nixos.nextcloud = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.nextcloud;
      appdata = "${config.neo.core.volumes.appdata}/nextcloud";
      domain = config.neo.services.swag.domain;
      nextcloudUrl = "${cfg.subdomain}.${domain}";
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-nextcloud-db.preStart = lib.neo.mkEnsureDirs config [
          {
            dirPath = "${appdata}/db";
            user = "999";
            group = "999";
          }
        ];
        systemd.services.docker-nextcloud.preStart = lib.neo.mkEnsureDirs config [
          {
            dirPath = "${appdata}/html";
            user = "33";
            group = "33";
          }
        ];
        # The db password reaches the containers through env files rendered at activation.
        neo.core.secrets.containerEnv = {
          nextcloud-db = {
            MYSQL_ROOT_PASSWORD = cfg.dbPassword;
            MYSQL_PASSWORD = cfg.dbPassword;
          };
          nextcloud.MYSQL_PASSWORD = cfg.dbPassword;
          nextcloud-cron.MYSQL_PASSWORD = cfg.dbPassword;
        };
        virtualisation.oci-containers.containers = {
          nextcloud-db = {
            image = cfg.containers."nextcloud-db";
            autoStart = true;
            cmd = [
              "--transaction-isolation=READ-COMMITTED"
              "--log-bin=binlog"
              "--binlog-format=ROW"
            ];
            environment = {
              MYSQL_DATABASE = "nextcloud";
              MYSQL_USER = "nextcloud";
            };
            volumes = [
              "${appdata}/db:/var/lib/mysql"
            ];
            networks = ["internal"];
          };

          nextcloud-redis = {
            image = cfg.containers."nextcloud-redis";
            autoStart = true;
            networks = ["internal"];
          };

          nextcloud = {
            image = cfg.containers."nextcloud";
            autoStart = true;
            environment = {
              MYSQL_HOST = "nextcloud-db";
              MYSQL_DATABASE = "nextcloud";
              MYSQL_USER = "nextcloud";
              REDIS_HOST = "nextcloud-redis";
              OVERWRITEPROTOCOL = "https";
              OVERWRITEHOST = nextcloudUrl;
              TRUSTED_PROXIES = "0.0.0.0/32";
              NEXTCLOUD_DEFAULT_GROUP = "all_users";
              TZ = config.neo.core.timeZone;
            };
            volumes = [
              "${appdata}/html:/var/www/html"
            ];
            networks = ["internal"];
          };

          nextcloud-cron = {
            image = cfg.containers."nextcloud-cron";
            autoStart = true;
            entrypoint = "/cron.sh";
            environment = {
              MYSQL_HOST = "nextcloud-db";
              MYSQL_DATABASE = "nextcloud";
              MYSQL_USER = "nextcloud";
              REDIS_HOST = "nextcloud-redis";
              OVERWRITEPROTOCOL = "https";
              OVERWRITEHOST = nextcloudUrl;
              TRUSTED_PROXIES = "0.0.0.0/32";
              NEXTCLOUD_DEFAULT_GROUP = "all_users";
              TZ = config.neo.core.timeZone;
            };
            volumes = [
              "${appdata}/html:/var/www/html"
            ];
            networks = ["internal"];
          };
        };

        # Configure Nextcloud via occ (lib.neo.mkSetupService, non-blocking).
        # Runs occ upgrade when needsDbUpgrade is set (image bumps), then
        # defaults/repair. Retries every 60s until done, then stays active
        # (exited). Requires= on the containers re-runs it after an image update.
        systemd.services.nextcloud-setup = lib.neo.mkSetupService {
          inherit pkgs;
          name = "nextcloud-setup";
          description = "Nextcloud post-install configuration (upgrade, maintenance window, defaults)";
          containers = ["nextcloud-db" "nextcloud-redis" "nextcloud"];
          attachTo = ["nextcloud"];
          retryInterval = 60;
          script = let
            docker = "${pkgs.docker}/bin/docker";
            occ = "${docker} exec --user www-data nextcloud php occ";
          in ''
            echo "Running Nextcloud setup..."
            if ${occ} status 2>/dev/null | grep -Eq 'needsDbUpgrade:[[:space:]]*true'; then
              echo "Pending Nextcloud DB upgrade detected, running occ upgrade..."
              ${occ} upgrade
              ${occ} maintenance:mode --off || true
            fi

            ${occ} config:system:set maintenance_window_start --value ${toString cfg.maintenanceWindowStart} --type integer
            ${occ} config:system:set default_phone_region --value '${cfg.defaultPhoneRegion}'
            ${occ} config:system:set instanceid --value '${cfg.instanceId}'
            ${occ} config:system:set overwritehost --value '${nextcloudUrl}'
            ${occ} config:system:set trusted_proxies 0 --value '0.0.0.0/0' --type string
            ${occ} maintenance:repair --include-expensive
            ${occ} db:add-missing-indices
            echo "Nextcloud setup completed."
          '';
        };
      };
    };
}
