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
            cfg.appdata
            "${cfg.appdata}/database"
            "${cfg.appdata}/config"
          ])
          (lib.neo.mkActivationScriptForFile config {
            filePath = "${cfg.appdata}/config/settings.json";
            content = settingsJson;
            mode = "0644";
          })
        ];

        virtualisation.oci-containers.containers.filebrowser = {
          environment = {
            TZ = config.neo.core.timeZone;
          };
          image = cfg.containers.filebrowser;
          volumes =
            [
              "${cfg.appdata}/config:/config"
              "${cfg.appdata}/database:/database"
              "${config.neo.core.volumes.media}:/srv/Media"
              "${config.neo.core.volumes.documents}:/srv/Documents"
              "${config.neo.core.volumes.appdata}:/srv/AppData"
            ]
            ++ lib.neo.toOciBindMounts cfg.additionalMountPoints;
          networks = [(lib.neo.docker.serviceNetwork "filebrowser")];
        };
      };
    };
}
