{
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
  imports = [
    ./option.nix
    ./swag.nix
  ];

  config = mkIf cfg.enabled {
    system.activationScripts.create-filebrowser-dirs = lib.concatStringsSep "\n" [
      (lib.neo.mkActivationScriptForDir config {
        dirPath = "${config.neo.volumes.appdata}/filebrowser";
      })
      (lib.neo.mkActivationScriptForDir config {
        dirPath = "${config.neo.volumes.appdata}/filebrowser/database";
      })
      (lib.neo.mkActivationScriptForDir config {
        dirPath = "${config.neo.volumes.appdata}/filebrowser/config";
      })
    ];

    system.activationScripts.filebrowser-settings = lib.neo.mkActivationScriptForFile config {
      filePath = "${config.neo.volumes.appdata}/filebrowser/config/settings.json";
      content = settingsJson;
      mode = "0644";
    };

    virtualisation.oci-containers.containers.filebrowser = {
      environment = {
        TZ = "Europe/Zurich";
      };
      image = "filebrowser/filebrowser:latest";
      autoStart = true;
      volumes =
        [
          "${config.neo.volumes.appdata}/filebrowser/config:/config"
          "${config.neo.volumes.appdata}/filebrowser/database:/database"
          "${config.neo.volumes.media}:/srv/Media"
          "${config.neo.volumes.documents}:/srv/Documents"
        ]
        ++ (lib.mapAttrsToList (
            hostVol: containerPath: "${config.neo.volumes.${hostVol}}:${containerPath}"
          )
          cfg.additionalMountPoints);
      extraOptions = [
        "--network=internal"
      ];
    };
  };
}
