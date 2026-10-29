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
              nextauthSecret = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = ''
                  Random secret used by Karakeep's built-in NextAuth to sign session JWTs (NEXTAUTH_SECRET).
                  Not a login password and not a substitute for tinyauth — generate once and keep stable.
                '';
                helper = lib.neo.helpers.randomToken;
              };
              meiliMasterKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 20;
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
                rank = 30;
                description = "Disable public signups, should only be activated if a user is already created.";
              };
              openaiApiKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 40;
                description = ''
                  API key for the OpenAI-compatible inference provider (OPENAI_API_KEY).
                  Defaults target xAI; leave empty to skip automatic AI tagging.
                '';
              };
              openaiBaseUrl = mkOption {
                type = types.str;
                default = "https://api.x.ai/v1";
                rank = 50;
                description = ''
                  OpenAI-compatible API base URL (OPENAI_BASE_URL).
                  Examples: https://api.x.ai/v1, https://api.openai.com/v1, http://ollama:11434/v1
                '';
              };
              inferenceTextModel = mkOption {
                type = types.str;
                default = "grok-latest";
                rank = 60;
                description = "Model for text inference / auto-tagging (INFERENCE_TEXT_MODEL)";
              };
              inferenceImageModel = mkOption {
                type = types.str;
                default = "grok-latest";
                rank = 70;
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
              "karakeep-chrome" = "zenika/alpine-chrome:124";
              "karakeep-meilisearch" = "getmeili/meilisearch:v1.41.0";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/karakeep"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = ./icon.svg;
              tagline = "Save links, notes and pictures to read later";
              description = ''
                Save anything from the web with one click in your browser or on your phone: articles, links, notes and images.
                Pages are archived so they stay readable even if they disappear, everything is searchable, and AI can tag it for you.
              '';
              notes = ''
                Formerly Hoarder. Crawls pages with a headless browser for screenshots and readable archives, and indexes them with Meilisearch.
                Automatic tagging and summaries need an OpenAI-compatible API key (default provider: xAI).
              '';
              replaces = ["Pocket" "Raindrop.io"];
              privacy = ["bookmarks"];
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
