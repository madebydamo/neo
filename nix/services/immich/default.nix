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
        systemd.services.docker-immich-server.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = "${config.neo.volumes.appdata}/immich/server";
        };
        systemd.services.docker-immich-machine-learning.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = "${config.neo.volumes.appdata}/immich/cache";
        };
        systemd.services.docker-immich-database.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = "${config.neo.volumes.appdata}/immich/data";
        };

        virtualisation.oci-containers.containers = {
          immich-server = {
            image = "ghcr.io/immich-app/immich-server:release";
            autoStart = true;
            environment = {
              DB_HOSTNAME = "immich-database";
              REDIS_HOSTNAME = "immich-redis";
            };
            volumes = [
              "${config.neo.volumes.appdata}/immich/server:/data"
              "/etc/localtime:/etc/localtime:ro"
            ];
            networks = ["internal"];
          };

          immich-machine-learning = {
            image = "ghcr.io/immich-app/immich-machine-learning:release";
            autoStart = true;
            volumes = [
              "${config.neo.volumes.appdata}/immich/cache:/cache"
            ];
            networks = ["internal"];
          };

          immich-redis = {
            image = "docker.io/valkey/valkey:8@sha256:81db6d39e1bba3b3ff32bd3a1b19a6d69690f94a3954ec131277b9a26b95b3aa";
            autoStart = true;
            extraOptions = [
              "--health-cmd=redis-cli ping"
              "--health-interval=10s"
              "--health-timeout=3s"
              "--health-retries=3"
            ];
            networks = ["internal"];
          };

          immich-database = {
            image = "ghcr.io/immich-app/postgres:14-vectorchord0.4.3-pgvectors0.2.0@sha256:bcf63357191b76a916ae5eb93464d65c07511da41e3bf7a8416db519b40b1c23";
            autoStart = true;
            environment = {
              POSTGRES_PASSWORD = "postgres";
              POSTGRES_USER = "postgres";
              POSTGRES_DB = "immich";
              POSTGRES_INITDB_ARGS = "--data-checksums";
            };
            volumes = [
              "${config.neo.volumes.appdata}/immich/data:/var/lib/postgresql/data"
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
