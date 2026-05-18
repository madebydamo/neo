# Beszel hub (server) implementation - tinyauth forwarded auth + optional single-user mode
{...}: {
  flake.modules.nixos.beszel = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.beszel;
    in {
      config = mkIf cfg.enabled {
        virtualisation.oci-containers.containers.beszel = {
          image = "henrygd/beszel:latest";
          autoStart = true;
          volumes = [
            "${config.neo.volumes.appdata}/beszel:/beszel_data"
          ];
          environment =
            {
              TZ = "Europe/Zurich";
              TRUSTED_AUTH_HEADER = "X-Tinyauth-User";
              APP_URL = "https://${cfg.subdomain}.${config.neo.services.swag.domain}";
            }
            // optionalAttrs cfg.enableSingleUserSystem {
              DISABLE_PASSWORD_AUTH = "true";
            };
          extraOptions = [
            "--network=internal"
          ];
        };

        systemd.services.docker-beszel.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = "${config.neo.volumes.appdata}/beszel";
        };
      };
    };
}
