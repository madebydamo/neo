# Vaultwarden service implementation.
{...}: {
  flake.modules.nixos.vaultwarden = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.vaultwarden;
      # Public origin: WebAuthn/FIDO2, attachments and email links all derive from DOMAIN.
      publicUrl =
        if (cfg.customDomains or []) != []
        then "https://${builtins.head cfg.customDomains}"
        else lib.neo.publicUrl config cfg;
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-vaultwarden.preStart = lib.neo.mkEnsureDirs config [
          cfg.appdata
        ];

        # The admin token reaches the container through an env file rendered at activation.
        neo.core.secrets.containerEnv.vaultwarden = mkIf (cfg.adminToken != null) {
          ADMIN_TOKEN = cfg.adminToken;
        };

        virtualisation.oci-containers.containers.vaultwarden = {
          image = cfg.containers.vaultwarden;
          environment =
            {
              ROCKET_ADDRESS = "0.0.0.0";
              ROCKET_PORT = toString cfg.port;
            }
            // optionalAttrs (publicUrl != null) {
              DOMAIN = publicUrl;
            };
          volumes = [
            "${cfg.appdata}:/data"
          ];
          networks = ["internal"];
        };
      };
    };
}
