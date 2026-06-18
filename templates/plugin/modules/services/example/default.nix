# Example service implementation.
{...}: {
  flake.modules.nixos.example = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.example;
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
        systemd.services.docker-example.preStart = lib.concatStringsSep "\n" [
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${config.neo.core.volumes.appdata}/example";
          })
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${config.neo.core.volumes.appdata}/example/database";
          })
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${config.neo.core.volumes.appdata}/example/config";
          })
          (lib.neo.mkActivationScriptForFile config {
            filePath = "${config.neo.core.volumes.appdata}/example/config/settings.json";
            content = settingsJson;
            mode = "0644";
          })
        ];

        virtualisation.oci-containers.containers.example = {
          environment = {
            TZ = "Europe/Zurich";
          };
          image = "filebrowser/filebrowser:latest";
          autoStart = true;
          volumes =
            [
              "${config.neo.core.volumes.appdata}/example/config:/config"
              "${config.neo.core.volumes.appdata}/example/database:/database"
              "${config.neo.core.volumes.media}:/srv/Media"
              "${config.neo.core.volumes.documents}:/srv/Documents"
            ]
            ++ (lib.mapAttrsToList (
                hostVol: containerPath: "${config.neo.core.volumes.${hostVol}}:${containerPath}"
              )
              cfg.additionalMountPoints);
          networks = ["internal"];
        };
      };
    };
}
