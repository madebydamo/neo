# Docmost service implementation (app + Postgres + Redis).
# UI behind tinyauth via SWAG; /api/health on publicPaths for probes.
# WebSockets for the real-time editor are covered by SWAG proxy.conf (do not re-set Upgrade/Connection).
{...}: {
  flake.modules.nixos.docmost = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.docmost;
      appdata = "${config.neo.core.volumes.appdata}/docmost";
      domain = config.neo.services.swag.domain;
      secretSet = v: v != null && v != "";
      dbPassword = cfg.dbPassword or "";
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = secretSet cfg.appSecret && (builtins.stringLength cfg.appSecret) >= 32;
            message = "neo.services.docmost: appSecret must be at least 32 characters when enabled.";
          }
          {
            assertion = secretSet cfg.dbPassword;
            message = "neo.services.docmost: dbPassword must be set when enabled.";
          }
        ];

        # Postgres 18 official image expects the volume at /var/lib/postgresql (not .../data).
        systemd.services.docker-docmost-db.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/pgdata"
        ];
        systemd.services.docker-docmost-redis.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/redisdata"
        ];
        systemd.services.docker-docmost.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/storage"
        ];

        virtualisation.oci-containers.containers = {
          docmost-redis = {
            image = cfg.containers."docmost-redis";
            autoStart = true;
            cmd = ["redis-server" "--appendonly" "yes" "--maxmemory-policy" "noeviction"];
            volumes = [
              "${appdata}/redisdata:/data"
            ];
            networks = ["internal"];
          };

          docmost-db = {
            image = cfg.containers."docmost-db";
            autoStart = true;
            environment = {
              POSTGRES_DB = "docmost";
              POSTGRES_USER = "docmost";
              POSTGRES_PASSWORD = dbPassword;
            };
            volumes = [
              "${appdata}/pgdata:/var/lib/postgresql"
            ];
            networks = ["internal"];
          };

          docmost = {
            image = cfg.containers.docmost;
            autoStart = true;
            environment = {
              APP_URL = "https://${cfg.subdomain}.${domain}";
              APP_SECRET = cfg.appSecret or "";
              DATABASE_URL = "postgresql://docmost:${dbPassword}@docmost-db:5432/docmost";
              REDIS_URL = "redis://docmost-redis:6379";
            };
            volumes = [
              "${appdata}/storage:/app/data/storage"
            ];
            networks = ["internal"];
          };
        };
      };
    };
}
