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
      appdata = cfg.appdata;
      domain = config.neo.services.swag.domain;
    in {
      config = mkIf cfg.enabled {
        assertions = lib.neo.requireSecrets "karakeep" cfg ["nextauthSecret" "meiliMasterKey"];

        systemd.services.docker-karakeep.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/data"
        ];
        systemd.services.docker-karakeep-meilisearch.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/meilisearch"
        ];

        # Secrets reach the containers through env files rendered at activation.
        neo.core.secrets.containerEnv = {
          karakeep-meilisearch.MEILI_MASTER_KEY = cfg.meiliMasterKey or "";
          karakeep =
            {
              # Strings coerced for env typing; assertions require non-empty when enabled.
              NEXTAUTH_SECRET = cfg.nextauthSecret or "";
              MEILI_MASTER_KEY = cfg.meiliMasterKey or "";
            }
            // optionalAttrs (lib.neo.isSet cfg.openaiApiKey) {
              OPENAI_API_KEY = cfg.openaiApiKey;
            };
        };

        virtualisation.oci-containers.containers = {
          karakeep-meilisearch = {
            image = cfg.containers."karakeep-meilisearch";
            environment = {
              MEILI_NO_ANALYTICS = "true";
              # Migrates the index in place when the image is a newer v1.x than the data.
              MEILI_UPGRADE_DB = "true";
            };
            volumes = [
              "${appdata}/meilisearch:/meili_data"
            ];
            networks = ["internal"];
          };

          karakeep-chrome = {
            image = cfg.containers."karakeep-chrome";
            # The image's entrypoint adds headless/no-sandbox and forwards :9222 to Chrome's
            # localhost DevTools port; remote-debugging flags here would clash with it.
            cmd = [
              "--disable-gpu"
              "--disable-dev-shm-usage"
              "--hide-scrollbars"
              "--disable-blink-features=AutomationControlled"
              "--window-size=1440,900"
            ];
            extraOptions = ["--init"];
            networks = ["internal"];
          };

          karakeep = {
            image = cfg.containers.karakeep;
            environment = {
              DATA_DIR = "/data";
              NEXTAUTH_URL = "https://${cfg.subdomain}.${domain}";
              MEILI_ADDR = "http://karakeep-meilisearch:7700";
              BROWSER_WEB_URL = "http://karakeep-chrome:9222";
              DISABLE_SIGNUPS = boolToString cfg.disableSignups;
              DB_WAL_MODE = "true";
              LOG_LEVEL = "warning";
              # OpenAI-compatible inference (default: xAI Grok).
              OPENAI_BASE_URL = cfg.openaiBaseUrl;
              INFERENCE_TEXT_MODEL = cfg.inferenceTextModel;
              INFERENCE_IMAGE_MODEL = cfg.inferenceImageModel;
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
