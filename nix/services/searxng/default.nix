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
        systemd.services.docker-searxng-redis.preStart = lib.concatStringsSep "\n" [
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}";
          })
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}/redis";
          })
        ];
        systemd.services.docker-searxng.preStart = lib.concatStringsSep "\n" [
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}";
          })
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}/searxng";
          })
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}/cache";
          })
        ];

        virtualisation.oci-containers.containers = {
          searxng-redis = {
            image = "docker.io/valkey/valkey:8-alpine";
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
            image = "docker.io/searxng/searxng:latest";
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
