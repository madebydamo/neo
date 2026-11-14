# Paperless service implementation (db, redis, main app). Web UI is behind tinyauth via swag proxy.
{...}: {
  flake.modules.nixos.paperless = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.paperless;
      appdata = cfg.appdata;
    in {
      config = mkIf cfg.enabled (mkMerge [
        (lib.neo.mkPostgresSidecar config {
          network = lib.neo.docker.serviceNetwork "paperless";
          name = "paperless-db";
          image = cfg.containers."paperless-db";
          inherit appdata;
          db = "paperless";
          user = "paperless_user";
          password = cfg.dbPassword;
        })
        (lib.neo.mkRedisSidecar config {
          network = lib.neo.docker.serviceNetwork "paperless";
          name = "paperless-redis";
          image = cfg.containers."paperless-redis";
          inherit appdata;
        })
        {
          # paperless-ngx 3.0+ raises ImproperlyConfigured without a real secret key.
          assertions = [
            {
              assertion = lib.neo.isSet cfg.secretKey && cfg.secretKey != "change-me";
              message = "neo.services.paperless: secretKey must be set to a unique value when enabled (use the Generate helper; not the default \"change-me\").";
            }
          ];

          systemd.services.docker-paperless.preStart = lib.neo.mkEnsureDirs config [
            "${appdata}/data"
            "${appdata}/media"
            "${appdata}/export"
            "${appdata}/consume"
          ];

          # Secrets reach the containers through env files rendered at activation.
          neo.core.secrets.containerEnv = {
            paperless = {
              PAPERLESS_DBPASS = cfg.dbPassword;
              PAPERLESS_SECRET_KEY = cfg.secretKey;
            };
          };

          virtualisation.oci-containers.containers = {
            paperless = {
              image = cfg.containers.paperless;
              environment = {
                PAPERLESS_REDIS = "redis://paperless-redis:6379";
                PAPERLESS_DBHOST = "paperless-db";
                PAPERLESS_DBNAME = "paperless";
                PAPERLESS_DBUSER = "paperless_user";
                PAPERLESS_TIME_ZONE = config.neo.core.timeZone;
                PAPERLESS_OCR_LANGUAGE = "deu+eng";
                PAPERLESS_CSRF_TRUSTED_ORIGINS = lib.neo.publicUrl config cfg;
              };
              volumes = [
                "${appdata}/data:/usr/src/paperless/data"
                "${appdata}/media:/usr/src/paperless/media"
                "${appdata}/export:/usr/src/paperless/export"
                "${appdata}/consume:/usr/src/paperless/consume"
              ];
              networks = [(lib.neo.docker.serviceNetwork "paperless")];
            };
          };
        }
      ]);
    };
}
