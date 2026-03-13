# Tinyauth forward authentication service implementation.
{...}: {
  flake.modules.nixos.tinyauth = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.tinyauth;
      domain = config.neo.services.swag.domain;

      # Collect all services that have auth enabled.
      authServices = filterAttrs (n: v: v.enabled && (v.auth.enabled or false)) config.neo.services;

      # Generate TINYAUTH_APPS_<NAME>_PATH_ALLOW env vars for services with publicPaths.
      appAclEnvVars =
        foldlAttrs (
          acc: name: svc:
            acc
            // (
              let
                appName = lib.toUpper (replaceStrings ["-"] ["_"] name);
              in
                optionalAttrs (svc.auth.publicPaths or [] != []) {
                  "TINYAUTH_APPS_${appName}_PATH_ALLOW" = "(${concatStringsSep "|" svc.auth.publicPaths})";
                }
                // optionalAttrs (svc.subdomain or null != null) {
                  "TINYAUTH_APPS_${appName}_CONFIG_DOMAIN" = "${svc.subdomain}.${domain}";
                }
            )
        ) {}
        authServices;
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = cfg.users != [];
            message = "neo.services.tinyauth: At least one user must be configured in 'users'.";
          }
        ];

        system.activationScripts.create-tinyauth-dirs = lib.concatStringsSep "\n" [
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${config.neo.volumes.appdata}/tinyauth";
          })
        ];

        virtualisation.oci-containers.containers.tinyauth = {
          image = "ghcr.io/steveiliop56/tinyauth:v5";
          autoStart = true;
          environment =
            {
              TINYAUTH_APPURL = "https://${cfg.subdomain}.${domain}";
              TINYAUTH_AUTH_USERS = concatStringsSep "," cfg.users;
              TINYAUTH_SERVER_PORT = toString cfg.port;
              TINYAUTH_AUTH_SESSIONEXPIRY = toString cfg.sessionExpiry;
              TINYAUTH_ANALYTICS_ENABLED = "false";
              TINYAUTH_DATABASE_PATH = "/data/tinyauth.db";
            }
            // appAclEnvVars;
          volumes = [
            "${config.neo.volumes.appdata}/tinyauth:/data"
          ];
          extraOptions = [
            "--network=internal"
          ];
        };
      };
    };
}
