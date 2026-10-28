# Dawarich service implementation (app, Sidekiq, PostGIS, Redis).
# UI behind tinyauth via SWAG. /api/ is on publicPaths for trackers and health.
# WebSockets (/cable) use SWAG proxy.conf Upgrade/Connection. Do not re-set them.
{...}: {
  flake.modules.nixos.dawarich = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.dawarich;
      appdata = "${config.neo.core.volumes.appdata}/dawarich";
      domain = config.neo.services.swag.domain or null;
      secretSet = v: v != null && v != "";
      inherit (lib.neo.secrets) isPlaceholder;
      dbUser = "dawarich";
      dbName = "dawarich";
      publicHost =
        if cfg.subdomain == null || domain == null
        then ""
        else "${cfg.subdomain}.${domain}";
      # Production rejects Host values that are not listed. Healthchecks use 127.0.0.1.
      applicationHosts = concatStringsSep "," (unique (
        ["localhost" "127.0.0.1" "::1" publicHost] ++ (cfg.customDomains or [])
      ));
      dbPassword =
        if secretSet cfg.dbPassword
        then cfg.dbPassword
        else "";
      secretKeyBase =
        if secretSet cfg.secretKeyBase
        then cfg.secretKeyBase
        else "";
      # Shared by the web process and the worker. Sidekiq adds its own concurrency.
      commonEnv = {
        RAILS_ENV = "production";
        RAILS_LOG_TO_STDOUT = "true";
        SELF_HOSTED = "true";
        STORE_GEODATA = "true";
        APPLICATION_PROTOCOL = "https";
        APPLICATION_HOSTS = applicationHosts;
        # Absolute links in mail (invites, password reset). No scheme, no slash.
        DOMAIN = publicHost;
        TIME_ZONE = config.neo.core.timeZone;
        REDIS_URL = "redis://dawarich-redis:6379";
        DATABASE_HOST = "dawarich-db";
        DATABASE_PORT = "5432";
        DATABASE_USERNAME = dbUser;
        DATABASE_NAME = dbName;
      };
      # Secret part of commonEnv: rendered env file per container (neo.core.secrets).
      commonSecretEnv = {
        SECRET_KEY_BASE = secretKeyBase;
        DATABASE_PASSWORD = dbPassword;
      };
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = publicHost != "";
            message = "neo.services.dawarich: subdomain and services.swag.domain must be set when enabled.";
          }
          {
            # An encrypted value is only checked for presence (its text is a placeholder).
            assertion = secretSet cfg.secretKeyBase && (isPlaceholder cfg.secretKeyBase || (cfg.secretKeyBase != "CHANGE_ME" && (stringLength cfg.secretKeyBase) >= 32));
            message = "neo.services.dawarich: secretKeyBase must be at least 32 characters when enabled (use the Generate helper; not CHANGE_ME).";
          }
          {
            assertion = secretSet cfg.dbPassword;
            message = "neo.services.dawarich: dbPassword must be set when enabled.";
          }
          {
            assertion = cfg.containers.dawarich == cfg.containers."dawarich-sidekiq";
            message = "neo.services.dawarich: containers.dawarich and containers.dawarich-sidekiq must be the same image tag.";
          }
        ];

        # Postgres 17 keeps its data directory at /var/lib/postgresql/data.
        systemd.services.docker-dawarich-db.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/pgdata"
        ];
        systemd.services.docker-dawarich-redis.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/redisdata"
        ];
        systemd.services.docker-dawarich.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/public"
          "${appdata}/storage"
          "${appdata}/watched"
        ];

        neo.core.secrets.containerEnv = {
          dawarich-db.POSTGRES_PASSWORD = dbPassword;
          dawarich = commonSecretEnv;
          dawarich-sidekiq = commonSecretEnv;
        };

        virtualisation.oci-containers.containers = {
          dawarich-redis = {
            image = cfg.containers."dawarich-redis";
            autoStart = true;
            # Upstream RDB snapshots. AOF is off so a crash can drop the newest jobs.
            cmd = [
              "redis-server"
              "--save"
              "900"
              "1"
              "--save"
              "300"
              "10"
              "--appendonly"
              "no"
            ];
            volumes = [
              "${appdata}/redisdata:/data"
            ];
            extraOptions = [
              "--health-cmd=redis-cli --raw incr ping"
              "--health-interval=10s"
              "--health-timeout=10s"
              "--health-retries=5"
              "--health-start-period=30s"
            ];
            networks = ["internal"];
          };

          dawarich-db = {
            image = cfg.containers."dawarich-db";
            autoStart = true;
            environment = {
              POSTGRES_DB = dbName;
              POSTGRES_USER = dbUser;
            };
            volumes = [
              "${appdata}/pgdata:/var/lib/postgresql/data"
            ];
            # Upstream shm_size. The official image enables PostGIS on POSTGRES_DB at first init.
            extraOptions = [
              "--shm-size=1g"
              "--health-cmd=pg_isready -U ${dbUser} -d ${dbName}"
              "--health-interval=10s"
              "--health-timeout=10s"
              "--health-retries=5"
              "--health-start-period=30s"
            ];
            networks = ["internal"];
          };

          dawarich = {
            image = cfg.containers.dawarich;
            autoStart = true;
            # Image ENTRYPOINT is `bundle exec`. The web script migrates, seeds, then execs the command.
            entrypoint = "web-entrypoint.sh";
            cmd = ["bin/rails" "server" "-p" "3000" "-b" "::"];
            dependsOn = ["dawarich-db" "dawarich-redis"];
            environment =
              commonEnv
              // {
                # One Puma worker. Upstream's household default.
                WEB_CONCURRENCY = "1";
              };
            volumes = [
              "${appdata}/public:/var/app/public"
              "${appdata}/watched:/var/app/tmp/imports/watched"
              "${appdata}/storage:/var/app/storage"
            ];
            extraOptions = [
              # APPLICATION_PROTOCOL=https turns on force_ssl, so plain HTTP is a 301.
              # The header is what SWAG sends. Without it wget follows https://127.0.0.1 and fails.
              "--health-cmd=wget -qO - --header=X-Forwarded-Proto:https http://127.0.0.1:3000/api/v1/health"
              "--health-interval=10s"
              "--health-timeout=10s"
              "--health-retries=30"
              "--health-start-period=60s"
            ];
            networks = ["internal"];
          };

          dawarich-sidekiq = {
            image = cfg.containers."dawarich-sidekiq";
            autoStart = true;
            # Script waits for Postgres, then runs Sidekiq. It ignores cmd.
            entrypoint = "sidekiq-entrypoint.sh";
            dependsOn = ["dawarich" "dawarich-db" "dawarich-redis"];
            environment =
              commonEnv
              // {
                BACKGROUND_PROCESSING_CONCURRENCY = "3";
              };
            volumes = [
              "${appdata}/public:/var/app/public"
              "${appdata}/watched:/var/app/tmp/imports/watched"
              "${appdata}/storage:/var/app/storage"
            ];
            networks = ["internal"];
          };
        };
      };
    };
}
