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
      # Always the app's own address: custom domains are extra or old names and,
      # with login on, redirect here (lib.neo.customDomainRedirects).
      publicUrl = lib.neo.publicUrl config cfg;

      tinyauth = config.neo.services.tinyauth;
      sso = cfg.sso;
      client = tinyauth.oidcClients.vaultwarden;
    in {
      config = mkIf cfg.enabled {
        # Uptime check: a path the app answers itself without a login (auth.publicPaths).
        neo.monitoring.apps.vaultwarden.uptimePath = "/api/alive";
        assertions = [
          {
            assertion = !sso || tinyauth.enabled;
            message = "neo.services.vaultwarden.sso: signing in with the login page needs tinyauth (services.tinyauth.enabled).";
          }
          {
            assertion = !sso || publicUrl != null;
            message = "neo.services.vaultwarden.sso: needs a public address (services.swag.domain).";
          }
        ];

        # tinyauth's OIDC server knows Vaultwarden as client "vaultwarden".
        neo.services.tinyauth.oidcClients = mkIf sso {
          vaultwarden = {
            name = "Vaultwarden";
            redirectUris = ["${publicUrl}/identity/connect/oidc-signin"];
          };
        };

        neo.core.containerDirs.vaultwarden = [
          cfg.appdata
        ];

        # The admin token reaches the container through an env file rendered at activation.
        neo.core.secrets.containerEnv.vaultwarden = mkMerge [
          (mkIf (cfg.adminToken != null) {ADMIN_TOKEN = cfg.adminToken;})
          (mkIf sso {SSO_CLIENT_SECRET = client.clientSecret;})
        ];

        virtualisation.oci-containers.containers.vaultwarden = {
          image = cfg.containers.vaultwarden;
          environment =
            {
              ROCKET_ADDRESS = "0.0.0.0";
              ROCKET_PORT = toString cfg.port;
            }
            // optionalAttrs (publicUrl != null) {
              DOMAIN = publicUrl;
            }
            // optionalAttrs sso {
              SSO_ENABLED = "true";
              SSO_ONLY = boolToString cfg.ssoOnly;
              SSO_AUTHORITY = tinyauth.oidcIssuer;
              SSO_CLIENT_ID = client.clientId;
              # Default scopes (email profile); tinyauth issues refresh tokens without offline_access.
            };
          volumes = [
            "${cfg.appdata}:/data"
          ];
        };
      };
    };
}
