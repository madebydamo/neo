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

        systemd.services."syncthing-config" = {
          after = ["docker-syncthing.service"];
          requires = ["docker-syncthing.service"];
          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
          };
          script = let
            configFile = "${config.neo.core.volumes.appdata}/syncthing/config.xml";
            uid = toString config.neo.core.uid;
            gid = toString config.neo.core.gid;
          in ''
            CONFIG_XML="${configFile}"
            for _ in $(seq 1 60); do
              if [ -f "$CONFIG_XML" ]; then
                break
              fi
              sleep 10
            done
            if [ ! -f "$CONFIG_XML" ]; then
              echo "syncthing config.xml not found after waiting"
              exit 1
            fi
            if grep -q '<insecureAdminAccess>true</insecureAdminAccess>' "$CONFIG_XML"; then
              echo "insecureAdminAccess already true"
            else
              sed -i '/<insecureAdminAccess>/d' "$CONFIG_XML" || true
              sed -i '/^[[:space:]]*<\/gui>/i\        <insecureAdminAccess>true</insecureAdminAccess>' "$CONFIG_XML"
              echo "insecureAdminAccess set to true"
            fi
            chown ${uid}:${gid} "$CONFIG_XML" || true
          '';
        };
        systemd.services.docker-syncthing.wants = ["syncthing-config.service"];
      };
    };
}
