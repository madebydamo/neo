# Stirling PDF service implementation (single container).
# Volumes and env match the production Docker Compose guide.
{...}: {
  flake.modules.nixos.stirling-pdf = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.stirling-pdf;
      appdata = cfg.appdata;
    in {
      config = mkIf cfg.enabled {
        # Uptime check: a path the app answers itself without a login (auth.publicPaths).
        neo.monitoring.apps.stirling-pdf.uptimePath = "/api/v1/info/status";
        neo.core.containerDirs.stirling-pdf = [
          appdata
          "${appdata}/tessdata"
          "${appdata}/configs"
          "${appdata}/logs"
          "${appdata}/customFiles"
          "${appdata}/pipeline"
        ];

        # The initial password reaches the container through an env file rendered at activation.
        neo.core.secrets.containerEnv."stirling-pdf" = mkIf cfg.enableLogin {
          SECURITY_INITIALLOGIN_PASSWORD = cfg.initialLoginPassword;
        };

        virtualisation.oci-containers.containers."stirling-pdf" = {
          image = cfg.containers."stirling-pdf";
          environment =
            {
              SECURITY_ENABLELOGIN = boolToString cfg.enableLogin;
              SYSTEM_DEFAULTLOCALE = cfg.defaultLocale;
              SYSTEM_GOOGLEVISIBILITY = boolToString cfg.googleVisibility;
              SYSTEMFILEUPLOADLIMIT = cfg.fileUploadLimit;
            }
            // optionalAttrs cfg.enableLogin {
              SECURITY_INITIALLOGIN_USERNAME = cfg.initialLoginUsername;
            };
          volumes = [
            "${appdata}/tessdata:/usr/share/tessdata"
            "${appdata}/configs:/configs"
            "${appdata}/logs:/logs"
            "${appdata}/customFiles:/customFiles:rw"
            "${appdata}/pipeline:/pipeline"
          ];
        };
      };
    };
}
