# Syncthing service implementation.
{...}: {
  flake.modules.nixos.syncthing = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.syncthing;
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-syncthing.preStart = lib.concatStringsSep "\n" [
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${config.neo.core.volumes.appdata}/syncthing";
          })
        ];

        virtualisation.oci-containers.containers.syncthing = {
          environment = {
            PUID = toString config.neo.core.uid;
            PGID = toString config.neo.core.gid;
            TZ = "Europe/Zurich";
          };
          image = "linuxserver/syncthing:latest";
          autoStart = true;
          volumes =
            [
              "${config.neo.core.volumes.appdata}/syncthing:/config"
              "${config.neo.core.volumes.data}:/DATA"
            ]
            ++ (lib.mapAttrsToList (
                hostVol: containerPath: "${config.neo.core.volumes.${hostVol}}:${containerPath}"
              )
              cfg.additionalMountPoints);
          ports = [
            "8384:8384"
            "22000:22000"
            "22000:22000/udp"
            "21027:21027/udp"
          ];
          networks = ["internal"];
        };
      };
    };
}
