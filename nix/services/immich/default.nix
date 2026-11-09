# Immich service implementation (server, ML, Redis, Postgres).
{...}: {
  flake.modules.nixos.immich = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.immich;
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-immich-server.preStart = lib.neo.mkEnsureDirs config [
          "${cfg.appdata}/server"
        ];
        systemd.services.docker-immich-machine-learning.preStart = lib.neo.mkEnsureDirs config [
          "${cfg.appdata}/cache"
        ];
        systemd.services.docker-immich-database.preStart = lib.neo.mkEnsureDirs config [
          "${cfg.appdata}/data"
        ];

        virtualisation.oci-containers.containers = {
          immich-server = {
            image = cfg.containers."immich-server";
            environment = {
              DB_HOSTNAME = "immich-database";
              REDIS_HOSTNAME = "immich-redis";
            };
            volumes = [
              "${cfg.appdata}/server:/data"
              "/etc/localtime:/etc/localtime:ro"
            ];
            networks = ["internal"];
          };

          immich-machine-learning = {
            image = cfg.containers."immich-machine-learning";
            volumes = [
              "${cfg.appdata}/cache:/cache"
            ];
            networks = ["internal"];
          };

          immich-redis = {
            image = cfg.containers."immich-redis";
            extraOptions = [
              "--health-cmd=redis-cli ping"
              "--health-interval=10s"
              "--health-timeout=3s"
              "--health-retries=3"
            ];
            networks = ["internal"];
          };

          immich-database = {
            image = cfg.containers."immich-database";
            environment = {
              POSTGRES_PASSWORD = "postgres";
              POSTGRES_USER = "postgres";
              POSTGRES_DB = "immich";
              POSTGRES_INITDB_ARGS = "--data-checksums";
            };
            volumes = [
              "${cfg.appdata}/data:/var/lib/postgresql/data"
            ];
            extraOptions = [
              "--shm-size=128mb"
            ];
            networks = ["internal"];
          };
        };
      };
    };
}
