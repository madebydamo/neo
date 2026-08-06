# Docmost (open-source collaborative wiki / Notion alternative) service options.
# Production layout: app + Postgres + Redis. UI behind tinyauth; /api/health for probes.
{...}: {
  flake.modules.nixos.docmost-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.docmost = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Docmost collaborative documentation platform" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 3000;
                description = "Internal port Docmost listens on (container port 3000)";
              };
              appSecret = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = ''
                  APP_SECRET: long random secret (32+ characters) used by Docmost for signing and encryption.
                  Generate once with the helper (openssl rand -hex 32) and keep stable; the app refuses to start with the default placeholder.
                '';
                helper = lib.neo.helpers.randomToken;
              };
              dbPassword = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 20;
                description = "Password for the internal Postgres container (POSTGRES_PASSWORD / DATABASE_URL)";
                helper = lib.neo.helpers.randomToken;
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "docmost";
              auth.publicPaths = [
                # Health probe for smoke tests and monitors (no session cookie).
                "^/api/health$"
              ];
            }
            // lib.neo.mkContainerDefinitions {
              docmost = "docmost/docmost:latest";
              "docmost-db" = "postgres:18";
              "docmost-redis" = "redis:8";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/docmost"
            // lib.neo.mkServiceMeta {
              category = "Files";
              icon = "https://raw.githubusercontent.com/docmost/docmost/refs/heads/main/apps/client/public/icons/app-icon-192x192.png";
              description = ''
                Docmost is an open-source collaborative wiki and knowledge base (Notion-style) with real-time page editing over WebSockets.
                Neo runs the official Docker stack: the Docmost app, Postgres for workspace data, and Redis for queues/sessions, all on the internal network behind SWAG and tinyauth.
                Set APP_URL to the public HTTPS URL so links and editor clients work; the health endpoint bypasses tinyauth while the UI stays protected.
                Ideal for self-hosted team docs without sending content to a SaaS wiki host.
              '';
              projectUrl = "https://docmost.com/docs";
              githubUrl = "https://github.com/docmost/docmost";
              releaseUrl = "https://github.com/docmost/docmost/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Docmost service configuration";
      };
    };
}
