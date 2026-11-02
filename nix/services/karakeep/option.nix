# Karakeep (bookmark / "hoarder") service options.
# App login still uses NextAuth internally (JWT secret required); edge SSO is tinyauth.
{...}: {
  flake.modules.nixos.karakeep-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      aiGroup = lib.neo.ui.mkGroup {
        id = "ai";
        label = "AI tagging";
        description = "Let an AI service tag and summarize your bookmarks automatically";
        icon = lib.neo.ui.icons.sparkles;
        rank = 50;
      };
    in {
      options.neo.services.karakeep = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Save links, notes and images in one place, with copies of pages kept for later" {rank = 0;};
              nextauthSecret = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                # NEXTAUTH_SECRET: signs NextAuth session JWTs.
                description = "Secret Karakeep uses to keep you signed in. Press Generate once and keep it: changing it signs everyone out.";
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "Session secret";};
              };
              meiliMasterKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 20;
                # MEILI_MASTER_KEY, shared by Karakeep and the Meilisearch container.
                description = "Secret for Karakeep's built-in search engine. Press Generate once and keep it: changing it breaks search.";
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "Search key";};
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
                description = "Stop new people from creating accounts. Turn this on after you have made yours.";
                ui = lib.neo.ui.mkUi {
                  label = "Disable sign-ups";
                };
              };
              openaiApiKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 40;
                # OPENAI_API_KEY for the OpenAI-compatible provider (default xAI).
                description = ''
                  Key from your AI service's account page (xAI by default) to tag bookmarks automatically.
                  Leave empty to turn AI tagging off.
                '';
                ui = lib.neo.ui.mkUi {
                  group = aiGroup;
                  label = "AI API key";
                };
              };
              openaiBaseUrl = mkOption {
                type = types.str;
                default = "https://api.x.ai/v1";
                rank = 50;
                # OPENAI_BASE_URL (OpenAI-compatible API).
                description = ''
                  Address of the AI service. The default is xAI.
                  Examples: https://api.openai.com/v1 for OpenAI, http://ollama:11434/v1 for your own Ollama.
                '';
                ui = lib.neo.ui.mkUi {
                  group = aiGroup;
                  label = "AI service address";
                  visibleWhen = "openaiApiKey";
                };
              };
              inferenceTextModel = mkOption {
                type = types.str;
                default = "grok-latest";
                rank = 60;
                # INFERENCE_TEXT_MODEL
                description = "AI model that reads pages and text to suggest tags.";
                ui = lib.neo.ui.mkUi {
                  group = aiGroup;
                  label = "Text model";
                  visibleWhen = "openaiApiKey";
                };
              };
              inferenceImageModel = mkOption {
                type = types.str;
                default = "grok-latest";
                rank = 70;
                # INFERENCE_IMAGE_MODEL
                description = "AI model that looks at images to suggest tags.";
                ui = lib.neo.ui.mkUi {
                  group = aiGroup;
                  label = "Image model";
                  visibleWhen = "openaiApiKey";
                };
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
              "karakeep-chrome" = "ghcr.io/karakeep-app/karakeep-chrome:release";
              "karakeep-meilisearch" = "getmeili/meilisearch:v1.55.0";
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
