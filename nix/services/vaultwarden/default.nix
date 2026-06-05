# Vaultwarden service implementation.
{...}: {
  flake.modules.nixos.vaultwarden = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.vaultwarden;
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-vaultwarden.preStart = lib.concatStringsSep "\n" [
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${config.neo.core.volumes.appdata}/vaultwarden";
          })
        ];

        virtualisation.oci-containers.containers.vaultwarden = {
          image = "vaultwarden/server:latest";
          autoStart = true;
          environment =
            {
              ROCKET_ADDRESS = "0.0.0.0";
              ROCKET_PORT = toString cfg.port;
            }
            // optionalAttrs (cfg.adminToken != null) {
              ADMIN_TOKEN = cfg.adminToken;
            };
          volumes = [
            "${config.neo.core.volumes.appdata}/vaultwarden:/data"
          ];
          networks = ["internal"];
        };
      };
    };
}
