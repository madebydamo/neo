# Paperless service implementation (db, redis, main app). Web UI is behind tinyauth via swag proxy.
{...}: {
  flake.modules.nixos.paperless = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.paperless;
      appdata = "${config.neo.volumes.appdata}/paperless";
      domain = config.neo.services.swag.domain;
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-paperless-db.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = "${appdata}/pgdata";
        };
        systemd.services.docker-paperless-redis.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = "${appdata}/redisdata";
        };
        systemd.services.docker-paperless.preStart = lib.concatStringsSep "\n" [
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}/data";
          })
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}/media";
          })
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}/export";
          })
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}/consume";
          })
        ];

        virtualisation.oci-containers.containers = {
          paperless-redis = {
            image = "redis:7";
            autoStart = true;
            volumes = [
              "${appdata}/redisdata:/data"
            ];
            extraOptions = [
              "--network=internal"
            ];
          };

          paperless-db = {
            image = "postgres:16";
            autoStart = true;
            environment = {
              POSTGRES_DB = "paperless";
              POSTGRES_USER = "paperless_user";
              POSTGRES_PASSWORD = cfg.dbPassword;
            };
            volumes = [
              "${appdata}/pgdata:/var/lib/postgresql/data"
            ];
            extraOptions = [
              "--network=internal"
            ];
          };

          paperless = {
            image = "ghcr.io/paperless-ngx/paperless-ngx:latest";
            autoStart = true;
            environment = {
              PAPERLESS_REDIS = "redis://paperless-redis:6379";
              PAPERLESS_DBHOST = "paperless-db";
              PAPERLESS_DBNAME = "paperless";
              PAPERLESS_DBUSER = "paperless_user";
              PAPERLESS_DBPASS = cfg.dbPassword;
              PAPERLESS_TIME_ZONE = config.neo.timeZone;
              PAPERLESS_OCR_LANGUAGE = "deu+eng";
              PAPERLESS_CSRF_TRUSTED_ORIGINS = "https://${cfg.subdomain}.${domain}";
            };
            volumes = [
              "${appdata}/data:/usr/src/paperless/data"
              "${appdata}/media:/usr/src/paperless/media"
              "${appdata}/export:/usr/src/paperless/export"
              "${appdata}/consume:/usr/src/paperless/consume"
            ];
            extraOptions = [
              "--network=internal"
            ];
          };
        };
      };
    };
}
