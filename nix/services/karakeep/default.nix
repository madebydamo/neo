# Karakeep service implementation (web app, headless Chrome, Meilisearch).
# Web UI is behind tinyauth via swag; /api is on publicPaths for extensions.
# NextAuth is built into Karakeep (session JWTs) — NEXTAUTH_* is still required.
{...}: {
  flake.modules.nixos.karakeep = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.karakeep;
      appdata = "${config.neo.core.volumes.appdata}/karakeep";
      domain = config.neo.services.swag.domain;
      secretSet = v: v != null && v != "";
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = secretSet cfg.nextauthSecret;
            message = "neo.services.karakeep: nextauthSecret must be set when enabled (use the Generate helper in the Neo UI).";
          }
          {
            assertion = secretSet cfg.meiliMasterKey;
            message = "neo.services.karakeep: meiliMasterKey must be set when enabled (use the Generate helper in the Neo UI).";
          }
        ];

        systemd.services.docker-karakeep.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/data"
        ];
        systemd.services.docker-karakeep-meilisearch.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/meilisearch"
        ];

        virtualisation.oci-containers.containers = {
          karakeep-meilisearch = {
            image = cfg.containers."karakeep-meilisearch";
            autoStart = true;
            environment = {
              MEILI_NO_ANALYTICS = "true";
              MEILI_MASTER_KEY = cfg.meiliMasterKey or "";
            };
            volumes = [
              "${appdata}/meilisearch:/meili_data"
            ];
            networks = ["internal"];
          };

          karakeep-chrome = {
            image = cfg.containers."karakeep-chrome";
            autoStart = true;
            cmd = [
              "--no-sandbox"
              "--disable-gpu"
              "--disable-dev-shm-usage"
              "--remote-debugging-address=0.0.0.0"
              "--remote-debugging-port=9222"
              "--hide-scrollbars"
              "--disable-blink-features=AutomationControlled"
              "--window-size=1440,900"
            ];
            networks = ["internal"];
          };

          karakeep = {
            image = cfg.containers.karakeep;
            autoStart = true;
            environment =
              {
                DATA_DIR = "/data";
                NEXTAUTH_URL = "https://${cfg.subdomain}.${domain}";
                # Strings coerced for oci-containers typing; assertions require non-empty when enabled.
                NEXTAUTH_SECRET = cfg.nextauthSecret or "";
                MEILI_ADDR = "http://karakeep-meilisearch:7700";
                MEILI_MASTER_KEY = cfg.meiliMasterKey or "";
                BROWSER_WEB_URL = "http://karakeep-chrome:9222";
                DISABLE_SIGNUPS = boolToString cfg.disableSignups;
                DB_WAL_MODE = "true";
                LOG_LEVEL = "warning";
                # OpenAI-compatible inference (default: xAI Grok).
                OPENAI_BASE_URL = cfg.openaiBaseUrl;
                INFERENCE_TEXT_MODEL = cfg.inferenceTextModel;
                INFERENCE_IMAGE_MODEL = cfg.inferenceImageModel;
              }
              // optionalAttrs (secretSet cfg.openaiApiKey) {
                OPENAI_API_KEY = cfg.openaiApiKey;
              };
            volumes = [
              "${appdata}/data:/data"
            ];
            networks = ["internal"];
          };
        };
      };
    };
}
