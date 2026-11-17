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
      config = mkIf cfg.enabled (mkMerge [
        (lib.neo.mkPostgresSidecar config {
          name = "immich-database";
          image = cfg.containers."immich-database";
          inherit (cfg) appdata;
          db = "immich";
          user = "postgres";
          dataDir = "data";
          # Not a secret: the database is only reachable on the internal network.
          environment = {
            POSTGRES_PASSWORD = "postgres";
            POSTGRES_INITDB_ARGS = "--data-checksums";
          };
          extraOptions = ["--shm-size=128mb"];
        })
        (lib.neo.mkRedisSidecar config {
          name = "immich-redis";
          image = cfg.containers."immich-redis";
          health = {
            interval = "10s";
            timeout = "3s";
            retries = 3;
          };
        })
        {
          neo.core.containerDirs.immich-server = [
            "${cfg.appdata}/server"
          ];
          neo.core.containerDirs.immich-machine-learning = [
            "${cfg.appdata}/cache"
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
            };

            immich-machine-learning = {
              image = cfg.containers."immich-machine-learning";
              volumes = [
                "${cfg.appdata}/cache:/cache"
              ];
            };
          };
        }
      ]);
    };
}
