# Activepieces service implementation (app + Postgres/pgvector + Redis).
# UI behind tinyauth via SWAG; /api/v1/webhooks on publicPaths for external triggers.
{...}: {
  flake.modules.nixos.activepieces = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.activepieces;
      appdata = "${config.neo.core.volumes.appdata}/activepieces";
      domain = config.neo.services.swag.domain;
      secretSet = v: v != null && v != "";
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = secretSet cfg.encryptionKey && builtins.match "[A-Za-z0-9]{32}" cfg.encryptionKey != null;
            message = "neo.services.activepieces: encryptionKey must be exactly 32 alphanumeric characters (openssl rand -hex 16).";
          }
          {
            assertion = secretSet cfg.jwtSecret;
            message = "neo.services.activepieces: jwtSecret must be set when enabled (use the Generate helper in the Neo UI).";
          }
          {
            assertion = secretSet cfg.dbPassword;
            message = "neo.services.activepieces: dbPassword must be set when enabled (use the Generate helper in the Neo UI).";
          }
        ];

        systemd.services.docker-activepieces-db.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = "${appdata}/pgdata";
        };
        systemd.services.docker-activepieces-redis.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = "${appdata}/redisdata";
        };
        systemd.services.docker-activepieces.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = "${appdata}/cache";
        };

        virtualisation.oci-containers.containers = {
          activepieces-redis = {
            image = cfg.containers."activepieces-redis";
            autoStart = true;
            volumes = [
              "${appdata}/redisdata:/data"
            ];
            networks = ["internal"];
          };

          activepieces-db = {
            image = cfg.containers."activepieces-db";
            autoStart = true;
            environment = {
              POSTGRES_DB = "activepieces";
              POSTGRES_USER = "activepieces";
              POSTGRES_PASSWORD = cfg.dbPassword or "";
            };
            volumes = [
              "${appdata}/pgdata:/var/lib/postgresql/data"
            ];
            networks = ["internal"];
          };

          activepieces = {
            image = cfg.containers.activepieces;
            autoStart = true;
            environment = {
              AP_ENVIRONMENT = "prod";
              AP_FRONTEND_URL = "https://${cfg.subdomain}.${domain}";
              AP_ENCRYPTION_KEY = cfg.encryptionKey or "";
              AP_JWT_SECRET = cfg.jwtSecret or "";
              AP_CONTAINER_TYPE = "WORKER_AND_APP";
              AP_EXECUTION_MODE = "UNSANDBOXED";
              AP_DB_TYPE = "POSTGRES";
              AP_POSTGRES_HOST = "activepieces-db";
              AP_POSTGRES_PORT = "5432";
              AP_POSTGRES_DATABASE = "activepieces";
              AP_POSTGRES_USERNAME = "activepieces";
              AP_POSTGRES_PASSWORD = cfg.dbPassword or "";
              AP_REDIS_TYPE = "STANDALONE";
              AP_REDIS_HOST = "activepieces-redis";
              AP_REDIS_PORT = "6379";
              AP_TELEMETRY_ENABLED = boolToString cfg.telemetryEnabled;
              # Queue UI needs credentials when enabled; leave off for a lean homeserver.
              AP_QUEUE_UI_ENABLED = "false";
              AP_PIECES_SYNC_MODE = "OFFICIAL_AUTO";
            };
            volumes = [
              "${appdata}/cache:/usr/src/app/cache"
            ];
            networks = ["internal"];
          };
        };
      };
    };
}
