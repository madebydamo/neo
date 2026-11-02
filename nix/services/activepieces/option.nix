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
              enabled = mkEnableOption "Automations that connect your apps, like a self-hosted Zapier" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 80;
                description = "Internal port Activepieces listens on (container port 80)";
              };
              # AP_ENCRYPTION_KEY: exactly 32 hex characters (openssl rand -hex 16); encrypts piece connections.
              encryptionKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = "Secret key that protects the app logins you save in Activepieces. Click Generate once and never change it, or you will have to reconnect every app.";
                ui = lib.neo.ui.mkUi {label = "Encryption key";};
                helper = lib.neo.helpers.randomToken32Hex;
              };
              # AP_JWT_SECRET: signs JWT session tokens.
              jwtSecret = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 20;
                description = "Secret that keeps you signed in. Click Generate once; changing it signs everyone out.";
                ui = lib.neo.ui.mkUi {label = "Session secret";};
                helper = lib.neo.helpers.randomToken;
              };
              # AP_POSTGRES_PASSWORD / POSTGRES_PASSWORD of the internal Postgres container.
              dbPassword = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 30;
                description = "Password for Activepieces's own database. Press Generate before the first start and keep it: changing it later cuts Activepieces off from its data.";
                ui = lib.neo.ui.mkUi {
                  label = "Database password";
                };
                helper = lib.neo.helpers.randomToken;
              };
              # AP_TELEMETRY_ENABLED
              telemetryEnabled = mkOption {
                type = types.bool;
                default = false;
                rank = 40;
                description = "Share anonymous usage statistics with the Activepieces developers.";
                ui = lib.neo.ui.mkUi {label = "Share usage statistics";};
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
              "activepieces-db" = "pgvector/pgvector:0.8.7-pg14";
              "activepieces-redis" = "redis:7-alpine";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/activepieces"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = ./icon.svg;
              tagline = "Automate tasks between your apps, like Zapier";
              description = ''
                Build automations without writing code: when something happens in one app, do something in another.
                For example, save email attachments to your files, or get a message when someone fills in a form.
                Automations can run on a schedule or when another app calls them.
              '';
              notes = ''
                Runs the official stack: the app (API and worker), Postgres with pgvector and Redis for the job queue.
                Neo sets AP_FRONTEND_URL to https://<subdomain>.<domain>, so webhook triggers and app logins (OAuth) work.
                Webhook paths skip the tinyauth login so other services can trigger flows; the editor stays behind it.
              '';
              replaces = ["Zapier" "Make"];
              privacy = ["automation"];
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
