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

  systemd.tmpfiles.rules = [
    "d ${config.neo.volumes.appdata}/filebrowser 0755 0 0 -"
  ];
}
// (mkIf cfg.enabled {
  virtualisation.oci-containers.containers.filebrowser = {
    user = "0:0";
    environment = {
      TZ = "Europe/Zurich";
      PUID = "0";
      PGID = "0";
    };
    image = "filebrowser/filebrowser:latest";
    autoStart = true;
    volumes = [
      "${config.neo.volumes.appdata}/filebrowser/filebrowser.json:/.filebrowser.json"
      "${config.neo.volumes.appdata}/filebrowser/filebrowser.db:/database.db"
    ]
    ++ [
      "${config.neo.volumes.media}:/srv/Media"
      "${config.neo.volumes.data}:/srv"
    ]
    ++ (lib.flatten (
      lib.attrValues (
        lib.mapAttrs (
          hostVol: containerPaths: lib.map (p: "${config.neo.volumes.${hostVol}}:${p}:Z") containerPaths
        ) cfg.additionalMountPoints
      )
    ));
    extraOptions = [
      "--network=internal"
    ];
  };
})
