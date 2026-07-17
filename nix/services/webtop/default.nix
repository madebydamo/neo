# Webtop (LinuxServer browser desktop) service implementation.
{...}: {
  flake.modules.nixos.webtop = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.webtop;
      appdata = "${config.neo.core.volumes.appdata}/webtop";
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-webtop.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = appdata;
        };

        virtualisation.oci-containers.containers.webtop = {
          image = cfg.containers.webtop;
          autoStart = true;
          environment = {
            PUID = toString config.neo.core.uid;
            PGID = toString config.neo.core.gid;
            TZ = config.neo.core.timeZone;
            TITLE = cfg.title;
          };
          volumes =
            [
              "${appdata}:/config"
            ]
            ++ (lib.mapAttrsToList (
                hostVol: containerPath: "${config.neo.core.volumes.${hostVol}}:${containerPath}"
              )
              cfg.additionalMountPoints);
          # Recommended by LinuxServer for desktop images (Chromium / Electron).
          extraOptions = [
            "--shm-size=1gb"
          ];
          networks = ["internal"];
        };
      };
    };
}
