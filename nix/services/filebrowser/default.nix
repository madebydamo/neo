# Filebrowser service implementation.
{...}: {
  flake.modules.nixos.filebrowser = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.filebrowser;
      settingsJson = builtins.toJSON {
        port = 8080;
        baseURL = "";
        address = "0.0.0.0";
        log = "stdout";
        database = "/database/filebrowser.db";
        root = "/srv";
      };
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-filebrowser.preStart = lib.concatStringsSep "\n" [
          (lib.neo.mkEnsureDirs config [
            "${config.neo.core.volumes.appdata}/filebrowser"
            "${config.neo.core.volumes.appdata}/filebrowser/database"
            "${config.neo.core.volumes.appdata}/filebrowser/config"
          ])
          (lib.neo.mkActivationScriptForFile config {
            filePath = "${config.neo.core.volumes.appdata}/filebrowser/config/settings.json";
            content = settingsJson;
            mode = "0644";
          })
        ];

        virtualisation.oci-containers.containers.filebrowser = {
          environment = {
            TZ = "Europe/Zurich";
          };
          image = cfg.containers.filebrowser;
          autoStart = true;
          volumes =
            [
              "${config.neo.core.volumes.appdata}/filebrowser/config:/config"
              "${config.neo.core.volumes.appdata}/filebrowser/database:/database"
              "${config.neo.core.volumes.media}:/srv/Media"
              "${config.neo.core.volumes.documents}:/srv/Documents"
              "${config.neo.core.volumes.appdata}:/srv/AppData"
            ]
            ++ lib.neo.toOciBindMounts cfg.additionalMountPoints;
          networks = ["internal"];
        };
      };
    };
}
