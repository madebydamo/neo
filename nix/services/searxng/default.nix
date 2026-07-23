# Searxng service implementation (redis + main app). Web UI is behind optional tinyauth via swag proxy.
{...}: {
  flake.modules.nixos.searxng = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.searxng;
      appdata = "${config.neo.core.volumes.appdata}/searxng";
      domain = config.neo.services.swag.domain;
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-searxng-redis.preStart = lib.neo.mkEnsureDirs config [
          appdata
          "${appdata}/redis"
        ];
        systemd.services.docker-searxng.preStart = lib.neo.mkEnsureDirs config [
          appdata
          "${appdata}/searxng"
          "${appdata}/cache"
        ];

        virtualisation.oci-containers.containers = {
          searxng-redis = {
            image = cfg.containers."searxng-redis";
            autoStart = true;
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
            autoStart = true;
            environment = {
              SEARXNG_BASE_URL = "https://${cfg.subdomain}.${domain}";
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
