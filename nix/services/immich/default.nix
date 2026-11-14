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
          network = lib.neo.docker.serviceNetwork "immich";
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
          network = lib.neo.docker.serviceNetwork "immich";
          name = "immich-redis";
          image = cfg.containers."immich-redis";
          health = {
            interval = "10s";
            timeout = "3s";
            retries = 3;
          };
        })
        {
          systemd.services.docker-immich-server.preStart = lib.neo.mkEnsureDirs config [
            "${cfg.appdata}/server"
          ];
          systemd.services.docker-immich-machine-learning.preStart = lib.neo.mkEnsureDirs config [
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
              networks = [(lib.neo.docker.serviceNetwork "immich")];
            };

            immich-machine-learning = {
              image = cfg.containers."immich-machine-learning";
              volumes = [
                "${cfg.appdata}/cache:/cache"
              ];
              networks = [(lib.neo.docker.serviceNetwork "immich")];
            };
          };
        }
      ]);
    };
}
