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
        systemd.services.docker-beszel.preStart = lib.neo.mkEnsureDirs config [
          cfg.appdata
        ];

        virtualisation.oci-containers.containers.beszel = {
          image = cfg.containers.beszel;
          volumes = [
            "${cfg.appdata}:/beszel_data"
          ];
          environment =
            {
              TZ = config.neo.core.timeZone;
              SHARE_ALL_SYSTEMS = "true";
              APP_URL = lib.neo.publicUrl config cfg;
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
