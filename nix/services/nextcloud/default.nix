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
      appdata = "${config.neo.volumes.appdata}/nextcloud";
      domain = config.neo.services.swag.domain;
      nextcloudUrl = "${cfg.subdomain}.${domain}";
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-nextcloud-db.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = "${appdata}/db";
          user = "999";
          group = "999";
        };
        systemd.services.docker-nextcloud.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = "${appdata}/html";
          user = "33";
          group = "33";
        };
        virtualisation.oci-containers.containers = {
          nextcloud-db = {
            image = "mariadb:10.6";
            autoStart = true;
            cmd = [
              "--transaction-isolation=READ-COMMITTED"
              "--log-bin=binlog"
              "--binlog-format=ROW"
            ];
            environment = {
              MYSQL_ROOT_PASSWORD = cfg.dbPassword;
              MYSQL_PASSWORD = cfg.dbPassword;
              MYSQL_DATABASE = "nextcloud";
              MYSQL_USER = "nextcloud";
            };
            volumes = [
              "${appdata}/db:/var/lib/mysql"
            ];
            networks = [ "internal" ];
          };

          nextcloud-redis = {
            image = "redis:alpine";
            autoStart = true;
            networks = [ "internal" ];
          };

          nextcloud = {
            image = "nextcloud:apache";
            autoStart = true;
            environment = {
              MYSQL_HOST = "nextcloud-db";
              MYSQL_DATABASE = "nextcloud";
              MYSQL_USER = "nextcloud";
              MYSQL_PASSWORD = cfg.dbPassword;
              REDIS_HOST = "nextcloud-redis";
              OVERWRITEPROTOCOL = "https";
              OVERWRITEHOST = nextcloudUrl;
              TRUSTED_PROXIES = "0.0.0.0/32";
              NEXTCLOUD_DEFAULT_GROUP = "all_users";
              TZ = config.neo.timeZone;
            };
            volumes = [
              "${appdata}/html:/var/www/html"
            ];
            networks = [ "internal" ];
          };

          nextcloud-cron = {
            image = "nextcloud:apache";
            autoStart = true;
            entrypoint = "/cron.sh";
            environment = {
              MYSQL_HOST = "nextcloud-db";
              MYSQL_DATABASE = "nextcloud";
              MYSQL_USER = "nextcloud";
              MYSQL_PASSWORD = cfg.dbPassword;
              REDIS_HOST = "nextcloud-redis";
              OVERWRITEPROTOCOL = "https";
              OVERWRITEHOST = nextcloudUrl;
              TRUSTED_PROXIES = "0.0.0.0/32";
              NEXTCLOUD_DEFAULT_GROUP = "all_users";
              TZ = config.neo.timeZone;
            };
            volumes = [
              "${appdata}/html:/var/www/html"
            ];
            networks = [ "internal" ];
          };
        };

        # Setup service to configure Nextcloud via occ. Retries every 60s until DB and container are ready.
        systemd.services.nextcloud-setup = {
          description = "Nextcloud post-install configuration (maintenance window, defaults)";
          after = [
            "docker-nextcloud-db.service"
            "docker-nextcloud-redis.service"
            "docker-nextcloud.service"
          ];
          requires = [
            "docker-nextcloud-db.service"
            "docker-nextcloud-redis.service"
            "docker-nextcloud.service"
          ];
          wants = ["docker-nextcloud.service"];
          wantedBy = ["multi-user.target"];
          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
            Restart = "on-failure";
            RestartSec = 60;
            StartLimitBurst = 30;
            StartLimitIntervalSec = "30min";
          };
          script = let
            docker = "${pkgs.docker}/bin/docker";
            occ = "${docker} exec --user www-data nextcloud php occ";
          in ''
            echo "Running Nextcloud setup..."
            # Fails (and systemd retries) if DB is not ready or container not running
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