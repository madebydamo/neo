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
      appdata = "${config.neo.core.volumes.appdata}/changedetection";
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-changedetection.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = appdata;
        };

        virtualisation.oci-containers.containers = {
          changedetection = {
            image = cfg.containers.changedetection;
            autoStart = true;
            environment = {
              PUID = toString config.neo.core.uid;
              PGID = toString config.neo.core.gid;
              TZ = config.neo.core.timeZone;
              WEBDRIVER_URL = "http://changedetection-webengine:4444/wd/hub";
              ALLOW_IANA_RESTRICTED_ADDRESSES = toString true;
            };
            volumes = [
              "${appdata}:/datastore"
            ];
            networks = ["internal"];
          };

          changedetection-webengine = {
            image = cfg.containers."changedetection-webengine";
            autoStart = true;
            environment = {
              PUID = toString config.neo.core.uid;
              PGID = toString config.neo.core.gid;
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
