# Syncthing service implementation.
{...}: {
  flake.modules.nixos.syncthing = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.syncthing;
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-syncthing.preStart = lib.neo.mkEnsureDirs config [
          "${config.neo.core.volumes.appdata}/syncthing"
        ];

        virtualisation.oci-containers.containers.syncthing = {
          environment = lib.neo.linuxserverEnv config;
          image = cfg.containers.syncthing;
          autoStart = true;
          volumes = [
            "${config.neo.core.volumes.appdata}/syncthing:/config"
            "${config.neo.core.volumes.data}:/DATA"
          ];
          # GUI (8384) is NOT published on the host — only reachable on the
          # Docker `internal` network via SWAG + tinyauth. Sync/discovery ports stay public.
          ports = [
            "22000:22000"
            "22000:22000/udp"
            "21027:21027/udp"
          ];
          networks = ["internal"];
        };

        # insecureAdminAccess lets SWAG (which strips Authorization) proxy the GUI
        # while tinyauth is the only user-facing gate on the public subdomain.
        # Setup unit (lib.neo.mkSetupService): retries every 10s until syncthing
        # has written config.xml.
        systemd.services."syncthing-config" = lib.neo.mkSetupService {
          inherit pkgs;
          name = "syncthing-config";
          description = "Allow SWAG to proxy the syncthing GUI (insecureAdminAccess)";
          containers = ["syncthing"];
          retryInterval = 10;
          script = let
            configFile = "${config.neo.core.volumes.appdata}/syncthing/config.xml";
            uid = toString config.neo.core.uid;
            gid = toString config.neo.core.gid;
          in ''
            CONFIG_XML="${configFile}"
            if [ ! -f "$CONFIG_XML" ]; then
              echo "syncthing config.xml not written yet"
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
      };
    };
}
