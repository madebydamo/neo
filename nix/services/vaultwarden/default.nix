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
        systemd.services.docker-vaultwarden.preStart = lib.neo.mkEnsureDirs config [
          "${config.neo.core.volumes.appdata}/vaultwarden"
        ];

        virtualisation.oci-containers.containers.vaultwarden = {
          image = cfg.containers.vaultwarden;
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
