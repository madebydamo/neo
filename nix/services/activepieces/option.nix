# Activepieces (open-source automation / Zapier alternative) service options.
# Production layout: app (WORKER_AND_APP) + Postgres (pgvector) + Redis.
# Webhooks bypass tinyauth via publicPaths; UI is behind edge auth.
{...}: {
  flake.modules.nixos.activepieces-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.activepieces = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Activepieces automation platform" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 80;
                description = "Internal port Activepieces listens on (container port 80)";
              };
              encryptionKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = ''
                  AP_ENCRYPTION_KEY: exactly 32 hex characters (16 bytes) used to encrypt connections.
                  Use Generate (or openssl rand -hex 16). Do not rotate after first start without re-saving all piece connections.
                '';
                helper = lib.neo.helpers.randomToken32Hex;
              };
              jwtSecret = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 20;
                description = ''
                  AP_JWT_SECRET: secret used to sign JWT session tokens.
                  Generate once (openssl rand -hex 32) and keep stable.
                '';
                helper = lib.neo.helpers.randomToken;
              };
              dbPassword = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 30;
                description = "Password for the internal Postgres container (AP_POSTGRES_PASSWORD / POSTGRES_PASSWORD)";
                helper = lib.neo.helpers.randomToken;
              };
              telemetryEnabled = mkOption {
                type = types.bool;
                default = false;
                rank = 40;
                description = "Send anonymous telemetry to Activepieces (AP_TELEMETRY_ENABLED)";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "activepieces";
              auth.publicPaths = [
                # Incoming webhooks / app triggers from third parties (no session cookie).
                "^/api/v1/webhooks"
              ];
            }
            // lib.neo.mkContainerDefinitions {
              activepieces = "ghcr.io/activepieces/activepieces:latest";
              "activepieces-db" = "pgvector/pgvector:0.8.0-pg14";
              "activepieces-redis" = "redis:7";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/activepieces"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/activepieces.svg";
              description = ''
                Activepieces is an open-source no-code automation platform (Zapier/Make alternative) for building flows that connect apps via pieces, webhooks, and schedules.
                Neo runs the official production stack: the app container (API + worker), Postgres with pgvector, and Redis for the job queue, all on the internal Docker network behind SWAG and tinyauth.
                Set AP_FRONTEND_URL to your public HTTPS URL so webhook triggers and OAuth redirects work; inbound webhook paths bypass tinyauth while the UI stays protected.
                Ideal for self-hosted home automations without sending credentials to a SaaS workflow host.
              '';
              projectUrl = "https://www.activepieces.com/docs";
              githubUrl = "https://github.com/activepieces/activepieces";
              releaseUrl = "https://github.com/activepieces/activepieces/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Activepieces service configuration";
      };
    };
}
