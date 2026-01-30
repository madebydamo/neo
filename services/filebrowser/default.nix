{ config, lib, ... }:

with lib;

let
  cfg = config.neo.services.filebrowser;
in
{
  imports = [
    ./option.nix
    ./swag.nix
  ];

  system.activationScripts.create-filebrowser-dirs = lib.concatStringsSep "\n" [
    (lib.neo.mkActivationScriptForDir {
      dirPath = "${config.neo.volumes.appdata}/filebrowser";
      user = "0";
      group = "0";
    })
    (lib.neo.mkActivationScriptForDir {
      dirPath = "${config.neo.volumes.appdata}/filebrowser/config";
      user = "1000";
      group = "1000";
    })
    (lib.neo.mkActivationScriptForDir {
      dirPath = "${config.neo.volumes.appdata}/filebrowser/database";
      user = "1000";
      group = "1000";
    })
  ];
}
// (mkIf cfg.enabled {
  virtualisation.oci-containers.containers.filebrowser = {
    user = "1000:1000";
    environment = {
      TZ = "Europe/Zurich";
      PUID = "1000";
      PGID = "1000";
    };
    image = "filebrowser/filebrowser:latest";
    autoStart = true;
    volumes = [
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
        ) cfg.additionalMountPoints
      )
    ));
    extraOptions = [
      "--network=internal"
    ];
  };
})
