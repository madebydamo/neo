# Calino service implementation (stateless static SPA).
# Web UI behind tinyauth via SWAG. No volumes: data lives in the browser.
{...}: {
  flake.modules.nixos.calino = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.calino;
    in {
      config = mkIf cfg.enabled {
        virtualisation.oci-containers.containers.calino = {
          image = cfg.containers.calino;
          environment = {
            TZ = config.neo.core.timeZone;
          };
          networks = [(lib.neo.docker.serviceNetwork "calino")];
        };
      };
    };
}
