# Firefox (LinuxServer browser via Selkies) service implementation.
{...}: {
  flake.modules.nixos.firefox = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.firefox;
      appdata = "${config.neo.core.volumes.appdata}/firefox";
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-firefox.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = appdata;
        };

        virtualisation.oci-containers.containers.firefox = {
          image = cfg.containers.firefox;
          autoStart = true;
          environment =
            {
              PUID = toString config.neo.core.uid;
              PGID = toString config.neo.core.gid;
              TZ = config.neo.core.timeZone;
              TITLE = cfg.title;
              # Unique from webtop (3050/3051/8082) and karakeep (3000); required when sharing gluetun.
              CUSTOM_PORT = toString cfg.port;
              CUSTOM_HTTPS_PORT = "3061";
              CUSTOM_WS_PORT = "8083";
            }
            // optionalAttrs (cfg.firefoxCli != "") {
              FIREFOX_CLI = cfg.firefoxCli;
            };
          volumes =
            [
              "${appdata}:/config"
            ]
            ++ (lib.mapAttrsToList (
                hostVol: containerPath: "${config.neo.core.volumes.${hostVol}}:${containerPath}"
              )
              cfg.additionalMountPoints);
          # Recommended by LinuxServer for modern sites (YouTube, etc.).
          extraOptions = [
            "--shm-size=1gb"
          ];
          networks = ["internal"];
        };
      };
    };
}
