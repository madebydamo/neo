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
      appdata = cfg.appdata;
      inherit (lib.neo.secrets) isPlaceholder;
    in {
      config = mkIf cfg.enabled (mkMerge [
        (lib.neo.mkPostgresSidecar config {
          name = "activepieces-db";
          image = cfg.containers."activepieces-db";
          inherit appdata;
          db = "activepieces";
          password = cfg.dbPassword or "";
        })
        (lib.neo.mkRedisSidecar config {
          name = "activepieces-redis";
          image = cfg.containers."activepieces-redis";
          inherit appdata;
        })
        {
          assertions =
            lib.neo.requireSecrets "activepieces" cfg ["jwtSecret" "dbPassword"]
            ++ [
              {
                # An encrypted value is only checked for presence (its text is a placeholder).
                assertion = lib.neo.isSet cfg.encryptionKey && (isPlaceholder cfg.encryptionKey || builtins.match "[A-Za-z0-9]{32}" cfg.encryptionKey != null);
                message = "neo.services.activepieces: encryptionKey must be exactly 32 alphanumeric characters (openssl rand -hex 16).";
              }
            ];

          systemd.services.docker-activepieces.preStart = lib.neo.mkEnsureDirs config [
            "${appdata}/cache"
          ];

          # Secrets reach the containers through env files rendered at activation.
          neo.core.secrets.containerEnv = {
            activepieces = {
              AP_ENCRYPTION_KEY = cfg.encryptionKey or "";
              AP_JWT_SECRET = cfg.jwtSecret or "";
              AP_POSTGRES_PASSWORD = cfg.dbPassword or "";
            };
          };

          virtualisation.oci-containers.containers = {
            activepieces = {
              image = cfg.containers.activepieces;
              environment = {
                AP_ENVIRONMENT = "prod";
                AP_FRONTEND_URL = lib.neo.publicUrl config cfg;
                AP_CONTAINER_TYPE = "WORKER_AND_APP";
                AP_EXECUTION_MODE = "UNSANDBOXED";
                AP_DB_TYPE = "POSTGRES";
                AP_POSTGRES_HOST = "activepieces-db";
                AP_POSTGRES_PORT = "5432";
                AP_POSTGRES_DATABASE = "activepieces";
                AP_POSTGRES_USERNAME = "activepieces";
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
        }
      ]);
    };
}
