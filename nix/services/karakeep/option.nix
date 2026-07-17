# Karakeep (bookmark / "hoarder") service options.
# App login still uses NextAuth internally (JWT secret required); edge SSO is tinyauth.
{...}: {
  flake.modules.nixos.karakeep-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.karakeep = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "karakeep bookmark and link archival service" {rank = 0;};
              nextauthSecret = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 1;
                description = ''
                  Random secret used by Karakeep's built-in NextAuth to sign session JWTs (NEXTAUTH_SECRET).
                  Not a login password and not a substitute for tinyauth — generate once and keep stable.
                '';
                helper = lib.neo.helpers.randomToken;
              };
              meiliMasterKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 2;
                description = ''
                  Meilisearch master key shared by Karakeep and the search container (MEILI_MASTER_KEY).
                  Generate once; rotating it without reindexing breaks search.
                '';
                helper = lib.neo.helpers.randomToken;
              };
              port = mkOption {
                type = types.port;
                internal = true;
                default = 3000;
                description = "Internal port for Karakeep web UI";
              };
              disableSignups = mkOption {
                type = types.bool;
                default = false;
                rank = 20;
                description = "Disable public signups, should only be activated if a user is already created.";
              };
              openaiApiKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 30;
                description = ''
                  API key for the OpenAI-compatible inference provider (OPENAI_API_KEY).
                  Defaults target xAI; leave empty to skip automatic AI tagging.
                '';
              };
              openaiBaseUrl = mkOption {
                type = types.str;
                default = "https://api.x.ai/v1";
                rank = 31;
                description = ''
                  OpenAI-compatible API base URL (OPENAI_BASE_URL).
                  Examples: https://api.x.ai/v1, https://api.openai.com/v1, http://ollama:11434/v1
                '';
              };
              inferenceTextModel = mkOption {
                type = types.str;
                default = "grok-latest";
                rank = 32;
                description = "Model for text inference / auto-tagging (INFERENCE_TEXT_MODEL)";
              };
              inferenceImageModel = mkOption {
                type = types.str;
                default = "grok-latest";
                rank = 33;
                description = "Model for image inference (INFERENCE_IMAGE_MODEL)";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "karakeep";
              auth.publicPaths = [
                # Browser extension, mobile apps, and REST API use Karakeep's own auth.
                "^/api/"
              ];
            }
            // lib.neo.mkVpnOptions {
              # Crawl + outbound AI/fetch traffic; meilisearch stays on internal only.
              containers = ["karakeep" "karakeep-chrome"];
              internalContainers = ["karakeep-meilisearch"];
              networks = ["internal"];
              # Web UI + Chrome remote debugging (shared gluetun netns; must not collide with other VPN services).
              ports = [3000 9222];
            }
            // lib.neo.mkContainerDefinitions {
              karakeep = "ghcr.io/karakeep-app/karakeep:release";
              "karakeep-chrome" = "gcr.io/zenika-hub/alpine-chrome:124";
              "karakeep-meilisearch" = "getmeili/meilisearch:v1.41.0";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/karakeep"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = "https://docs.karakeep.app/img/logo-full.svg";
              description = ''
                Karakeep is a self-hosted bookmark manager (formerly Hoarder) for hoarding links, notes, and images with full-text search and AI-assisted tagging.
                It crawls pages with a headless browser for screenshots and readable archives, indexes content with Meilisearch, and supports browser extensions and mobile apps for quick capture.
                Optional OpenAI-compatible inference (default: xAI Grok) enables automatic tagging and summarization of saved content.
                Ideal for building a private, searchable knowledge base of everything you want to keep from the web.
              '';
              projectUrl = "https://docs.karakeep.app/";
              githubUrl = "https://github.com/karakeep-app/karakeep";
              releaseUrl = "https://github.com/karakeep-app/karakeep/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Karakeep service configuration";
      };
    };
}
