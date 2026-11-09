# Changedetection service implementation (main + sockpuppetbrowser Chrome).
# Web UI is behind tinyauth via the swag proxy config.
{...}: {
  flake.modules.nixos.changedetection = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.changedetection;
      appdata = cfg.appdata;
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-changedetection.preStart = lib.neo.mkEnsureDirs config [appdata];

        virtualisation.oci-containers.containers = {
          changedetection = {
            image = cfg.containers.changedetection;
            environment =
              lib.neo.linuxserverEnv config
              // {
                PLAYWRIGHT_DRIVER_URL = "ws://changedetection-webengine:4444";
                ALLOW_IANA_RESTRICTED_ADDRESSES = toString true;
              };
            volumes = [
              "${appdata}:/datastore"
            ];
            networks = ["internal"];
          };

          changedetection-webengine = {
            image = cfg.containers."changedetection-webengine";
            # Off the default 3000/8080, which karakeep and searxng use inside the shared VPN namespace.
            cmd = ["/usr/local/bin/entrypoint.sh" "--port" "4444" "--sport" "4445"];
            extraOptions = [
              "--shm-size=2gb"
            ];
            networks = ["internal"];
          };
        };
      };
    };
}
