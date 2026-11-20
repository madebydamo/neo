# ConvertX service implementation (single container).
# Data lives at /app/data. PUID/PGID are applied by the image entrypoint (gosu).
# Session cookies are Secure because HTTP_ALLOWED is left unset (off).
{...}: {
  flake.modules.nixos.convertx = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.convertx;
      appdata = cfg.appdata;
    in {
      config = mkIf cfg.enabled {
        assertions =
          lib.neo.requireSecrets "convertx" cfg [
            {
              name = "sessionSecret";
              minLength = 16;
            }
          ]
          ++ [
            {
              assertion = cfg.auth.enabled || !cfg.allowUnauthenticated;
              message = "neo.services.convertx: Skip the app's own login needs Require login, otherwise anyone who can open the address can convert files.";
            }
          ];

        neo.core.containerDirs.convertx = [appdata];

        # JWT_SECRET reaches the container through an env file rendered at activation.
        neo.core.secrets.containerEnv.convertx = mkIf (lib.neo.isSet cfg.sessionSecret) {
          JWT_SECRET = cfg.sessionSecret;
        };

        virtualisation.oci-containers.containers.convertx = {
          image = cfg.containers.convertx;
          environment =
            lib.neo.linuxserverEnv config
            // {
              PORT = toString cfg.port;
              ACCOUNT_REGISTRATION = boolToString cfg.allowRegistration;
              ALLOW_UNAUTHENTICATED = boolToString cfg.allowUnauthenticated;
              AUTO_DELETE_EVERY_N_HOURS = toString cfg.keepHours;
              HIDE_HISTORY = boolToString cfg.hideHistory;
              LANGUAGE = cfg.language;
              MAX_CONVERT_PROCESS = toString cfg.maxConversions;
            };
          volumes = [
            "${appdata}:/app/data"
          ];
          extraOptions = lib.neo.dockerHealthOptions {
            cmd = "curl -fsS http://127.0.0.1:${toString cfg.port}/healthcheck";
            interval = "30s";
            timeout = "5s";
            retries = 5;
            startPeriod = "40s";
          };
        };
      };
    };
}
