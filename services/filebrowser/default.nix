{
  config,
  lib,
  ...
}:
with lib; let
  cfg = config.neo.services.filebrowser;
in
  {
    imports = [
      ./option.nix
      ./swag.nix
    ];

    system.activationScripts.create-filebrowser-dirs = lib.concatStringsSep "\n" [
      (lib.neo.mkActivationScriptForDir config {
        dirPath = "${config.neo.volumes.appdata}/filebrowser";
      })
      (lib.neo.mkActivationScriptForDir config {
        dirPath = "${config.neo.volumes.appdata}/filebrowser/database";
      })
      (lib.neo.mkActivationScriptForFile config {
        dirPath = "${config.neo.volumes.appdata}/filebrowser/config";
      })
    ];
  }
  // (mkIf cfg.enabled {
    virtualisation.oci-containers.containers.filebrowser = {
      user = "${toString config.neo.uid}:${toString config.neo.gid}";
      environment = {
        TZ = "Europe/Zurich";
        PUID = toString config.neo.uid;
        PGID = toString config.neo.gid;
      };
      image = "filebrowser/filebrowser:latest";
      autoStart = true;
      volumes =
        [
          "${config.neo.volumes.appdata}/filebrowser/config:/config"
          "${config.neo.volumes.appdata}/filebrowser/database:/database"
        ]
        ++ [
          "${config.neo.volumes.media}:/srv/Media"
          "${config.neo.volumes.documents}:/srv/Documents"
        ]
        ++ (lib.flatten (
          lib.attrValues (
            lib.mapAttrs (
              hostVol: containerPaths: lib.map (p: "${config.neo.volumes.${hostVol}}:${p}") containerPaths
            )
            cfg.additionalMountPoints
          )
        ));
      extraOptions = [
        "--network=internal"
      ];
    };
  })
