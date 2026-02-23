{
  config,
  lib,
  ...
}:
with lib; let
  cfg = config.neo.services.immich;
in {
  imports = [
    ./option.nix
    ./swag.nix
  ];

  config = mkIf cfg.enabled {
    system.activationScripts.create-immich-dirs = lib.concatStringsSep "\n" [
      (lib.neo.mkActivationScriptForDir config {
        dirPath = "${config.neo.volumes.appdata}/immich";
      })
      (lib.neo.mkActivationScriptForDir config {
        dirPath = "${config.neo.volumes.appdata}/immich/server";
      })
      (lib.neo.mkActivationScriptForDir config {
        dirPath = "${config.neo.volumes.appdata}/immich/cache";
      })
      (lib.neo.mkActivationScriptForDir config {
        dirPath = "${config.neo.volumes.appdata}/immich/data";
      })
    ];

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
        extraOptions = [
          "--network=internal"
        ];
      };

      immich-machine-learning = {
        image = "ghcr.io/immich-app/immich-machine-learning:release";
        autoStart = true;
        volumes = [
          "${config.neo.volumes.appdata}/immich/cache:/cache"
        ];
        extraOptions = [
          "--network=internal"
        ];
      };

      immich-redis = {
        image = "docker.io/valkey/valkey:8@sha256:81db6d39e1bba3b3ff32bd3a1b19a6d69690f94a3954ec131277b9a26b95b3aa";
        autoStart = true;
        extraOptions = [
          "--network=internal"
          "--health-cmd=redis-cli ping"
          "--health-interval=10s"
          "--health-timeout=3s"
          "--health-retries=3"
        ];
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
          "--network=internal"
          "--shm-size=128mb"
        ];
      };
    };
  };
}
