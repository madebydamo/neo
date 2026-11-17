# Searxng service implementation (redis + main app). Web UI is behind optional tinyauth via swag proxy.
{...}: {
  flake.modules.nixos.searxng = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.searxng;
      appdata = cfg.appdata;
    in {
      config = mkIf cfg.enabled {
        neo.core.containerDirs.searxng-redis = [
          appdata
          "${appdata}/redis"
        ];
        neo.core.containerDirs.searxng = [
          appdata
          "${appdata}/searxng"
          "${appdata}/cache"
        ];

        virtualisation.oci-containers.containers = {
          searxng-redis = {
            image = cfg.containers."searxng-redis";
            cmd = [
              "valkey-server"
              "--save"
              "30"
              "1"
              "--loglevel"
              "warning"
            ];
            volumes = [
              "${appdata}/redis:/data"
            ];
          };

          searxng = {
            image = cfg.containers.searxng;
            environment = {
              SEARXNG_BASE_URL = lib.neo.publicUrl config cfg;
              TZ = config.neo.core.timeZone;
            };
            volumes = [
              "${appdata}/searxng:/etc/searxng:rw"
              "${appdata}/cache:/var/cache/searxng:rw"
            ];
          };
        };
      };
    };
}
