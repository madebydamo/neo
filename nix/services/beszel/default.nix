# Beszel monitoring service implementation.
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
        systemd.services.docker-beszel.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = "${config.neo.volumes.appdata}/beszel";
        };

        virtualisation.oci-containers.containers.beszel = {
          image = "henrygd/beszel:latest";
          autoStart = true;
          volumes = [
            "${config.neo.volumes.appdata}/beszel:/beszel_data"
          ];
          environment =
            {
              TZ = "Europe/Zurich";
              SHARE_ALL_SYSTEMS = "true";
              APP_URL = "https://${cfg.subdomain}.${config.neo.services.swag.domain}";
            }
            // optionalAttrs cfg.enableSingleUserSystem {
              AUTO_LOGIN = "not-that-important@example.com";
              USER_EMAIL = "not-that-important@example.com";
              USER_PASSWORD = "whatever";
            };
          networks = ["internal"];
        };
      };
    };
}

