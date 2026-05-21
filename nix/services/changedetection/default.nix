# Changedetection service implementation (main + selenium webengine).
# Web UI is behind tinyauth via the swag proxy config.
{...}: {
  flake.modules.nixos.changedetection = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.changedetection;
      appdata = "${config.neo.volumes.appdata}/changedetection";
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-changedetection.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = appdata;
        };

        virtualisation.oci-containers.containers = {
          changedetection = {
            image = "dgtlmoon/changedetection.io";
            autoStart = true;
            environment = {
              PUID = toString config.neo.uid;
              PGID = toString config.neo.gid;
              TZ = config.neo.timeZone;
              WEBDRIVER_URL = "http://changedetection-webengine:4444/wd/hub";
            };
            volumes = [
              "${appdata}:/datastore"
            ];
            networks = ["internal"];
          };

          changedetection-webengine = {
            image = "selenium/standalone-chrome-debug:3.141.59";
            autoStart = true;
            environment = {
              PUID = toString config.neo.uid;
              PGID = toString config.neo.gid;
            };
            extraOptions = [
              "--shm-size=2gb"
            ];
            networks = ["internal"];
          };
        };
      };
    };
}
