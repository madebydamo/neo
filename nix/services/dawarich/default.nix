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
      appdata = cfg.appdata;
      domain = config.neo.services.swag.domain or null;
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
        if lib.neo.isSet cfg.dbPassword
        then cfg.dbPassword
        else "";
      secretKeyBase =
        if lib.neo.isSet cfg.secretKeyBase
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
      config = mkIf cfg.enabled (mkMerge [
        (lib.neo.mkPostgresSidecar config {
          name = "dawarich-db";
          image = cfg.containers."dawarich-db";
          inherit appdata;
          db = dbName;
          user = dbUser;
          password = dbPassword;
          # Upstream shm_size. The official image enables PostGIS on POSTGRES_DB at first init.
          extraOptions = ["--shm-size=1g"];
          health = {
            interval = "10s";
            timeout = "10s";
            retries = 5;
            startPeriod = "30s";
          };
        })
        (lib.neo.mkRedisSidecar config {
          name = "dawarich-redis";
          image = cfg.containers."dawarich-redis";
          inherit appdata;
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
          health = {
            cmd = "redis-cli --raw incr ping";
            interval = "10s";
            timeout = "10s";
            retries = 5;
            startPeriod = "30s";
          };
        })
        {
          assertions =
            lib.neo.requireSecrets "dawarich" cfg ["dbPassword"]
            ++ [
              {
                assertion = publicHost != "";
                message = "neo.services.dawarich: subdomain and services.swag.domain must be set when enabled.";
              }
              {
                # An encrypted value is only checked for presence (its text is a placeholder).
                assertion = lib.neo.isSet cfg.secretKeyBase && (cfg.secretKeyBase != "CHANGE_ME" && lib.neo.secrets.isLongEnough 32 cfg.secretKeyBase);
                message = "neo.services.dawarich: secretKeyBase must be at least 32 characters when enabled (use the Generate helper; not CHANGE_ME).";
              }
              {
                assertion = cfg.containers.dawarich == cfg.containers."dawarich-sidekiq";
                message = "neo.services.dawarich: containers.dawarich and containers.dawarich-sidekiq must be the same image tag.";
              }
            ];

          systemd.services.docker-dawarich.preStart = lib.neo.mkEnsureDirs config [
            "${appdata}/public"
            "${appdata}/storage"
            "${appdata}/watched"
          ];

          neo.core.secrets.containerEnv = {
            dawarich = commonSecretEnv;
            dawarich-sidekiq = commonSecretEnv;
          };

          virtualisation.oci-containers.containers = {
            dawarich = {
              image = cfg.containers.dawarich;
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
        }
      ]);
    };
}
