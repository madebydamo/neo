# Hermes service options.
# Order: enabled → required secrets → Telegram → llm (provider/key/model) → soul/supervise → proxy/skill.
{...}: {
  flake.modules.nixos.hermes-option = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      neoHermesAuth = lib.neo.mkNeoHermesAuth pkgs ./oauth.py;
    in {
      options.neo.services.hermes = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Hermes Agent service" {rank = 0;};

              gatewayPort = mkOption {
                type = types.port;
                internal = true;
                default = 18789;
                description = "Port for the Hermes gateway/API";
              };

              dashboardPort = mkOption {
                type = types.port;
                internal = true;
                default = 9119;
                description = "Port for the Hermes web dashboard UI (uses tinyauth via SWAG)";
              };

              dashboardPassword = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = ''
                  Internal Hermes dashboard password (required for non-loopback bind).
                  SWAG auto-logs in with this so only tinyauth is user-facing.
                '';
                rank = 10;
                helper = lib.neo.helpers.randomToken // {label = "Generate dashboard password";};
              };

              gatewayToken = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Gateway authentication token";
                rank = 20;
                helper = lib.neo.helpers.randomToken;
              };

              telegramBotToken = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = ''
                  Telegram bot token string.
                  Create a bot via @BotFather on Telegram.
                '';
                rank = 30;
              };

              telegramAllowedUserId = mkOption {
                type = types.listOf types.int;
                default = [];
                description = ''
                  List of Telegram user/chat IDs allowed to interact with the bot.
                  The first ID is the home channel (TELEGRAM_HOME_CHANNEL) used for
                  `hermes send --to telegram` and cron delivery. Group chat IDs are negative.
                  Get your ID from @userinfobot on Telegram.
                '';
                rank = 40;
                ui = lib.neo.ui.mkUi {
                  widget = "primaryItemList";
                  entryLabel = "Home channel";
                  emptyHint = "Add a Telegram user or chat ID. The first entry is the home channel.";
                };
              };

              telegramGroups = mkOption {
                type = types.attrsOf (
                  types.submodule {
                    options = {
                      requireMention = mkOption {
                        type = types.bool;
                        default = true;
                        rank = 0;
                        description = "Whether the bot requires an @mention in this group";
                      };
                    };
                  }
                );
                rank = 50;
                default = {};
                description = ''
                  Per-group Telegram settings. Keys are chat IDs (as strings) or "*" for default.
                '';
              };

              # One provider + key or OAuth + model. Catalog: hermes-agent plugins/model-providers.
              llm = mkOption {
                type = types.submodule {
                  options = {
                    provider = mkOption {
                      type = types.nullOr (types.enum lib.neo.hermesProviderIds);
                      default = null;
                      apply = v:
                        if v == ""
                        then null
                        else v;
                      rank = 0;
                      description = ''
                        Inference provider written to config.yaml as model.provider.
                        The list is hermes-agent model-provider plugins plus Hermes overlays
                        that have no plugin (SuperGrok is xai-oauth). API-key vendors: paste
                        llm.apiKey. OAuth vendors (openai-codex ChatGPT/Codex, xai-oauth
                        SuperGrok, nous, anthropic Claude, …): Log in below (writes auth.json
                        as user hermes). Custom / Ollama / vLLM: pick custom, set baseUrl, and
                        paste an API key if the endpoint needs one.
                      '';
                    };

                    apiKey = mkOption {
                      type = types.nullOr types.str;
                      default = null;
                      rank = 10;
                      example = "xai-...";
                      description = ''
                        API key for the selected provider. For named vendors Neo maps it to
                        that plugin's env var (XAI_API_KEY, ANTHROPIC_API_KEY, HF_TOKEN, …).
                        For provider = custom it is written to config.yaml as model.api_key
                        (Ollama often needs none; vLLM / OpenAI-compatible proxies often do).
                        Leave empty when using OAuth, AWS, Vertex, or a keyless endpoint.
                      '';
                    };

                    model = mkOption {
                      type = types.nullOr types.str;
                      default = "grok-build-latest";
                      rank = 20;
                      example = "grok-4.6";
                      description = ''
                        Default LLM model id written to config.yaml as model.default.
                        Use the suggested list for the selected provider, or paste any id
                        the endpoint serves (e.g. grok-4.6, claude-sonnet-4,
                        anthropic/claude-sonnet-4 on OpenRouter, llama3.2 on Ollama).
                        Leave empty so Nix does not pin the model. Under Hermes NixOS managed
                        mode the dashboard cannot save model changes — pin here.
                      '';
                    };

                    baseUrl = mkOption {
                      type = types.nullOr types.str;
                      default = null;
                      rank = 30;
                      example = "http://127.0.0.1:11434/v1";
                      description = ''
                        OpenAI-compatible API base URL for provider = custom (Ollama, vLLM,
                        llama.cpp, or any /v1 proxy). Written to model.base_url. Leave empty
                        for built-in provider endpoints. Example: Ollama
                        http://127.0.0.1:11434/v1
                      '';
                    };
                  };
                };
                default = {};
                rank = 60;
                description = "LLM provider, API key or OAuth login, and default model";
                ui = lib.neo.ui.mkUi {
                  widget = "providerAuth";
                  catalog = lib.neo.hermesProviderCatalog;
                  oauth = lib.neo.ui.mkOauth {
                    script = "${neoHermesAuth}/bin/neo-hermes-auth";
                    runAs = "hermes";
                    env = {
                      HERMES_HOME = "${config.neo.services.hermes.stateDir}/.hermes";
                    };
                  };
                };
              };

              forceSoul = mkOption {
                type = types.bool;
                default = false;
                rank = 88;
                description = ''
                  When true, overwrite $HERMES_HOME/SOUL.md with Neo's default co-pilot identity
                  on every activation. When false (default), seed SOUL.md only if missing so
                  operator edits are preserved.
                '';
              };

              superviseUpdates = mkOption {
                type = types.bool;
                default = false;
                rank = 89;
                description = ''
                  After a system-updater or docker-updater run that actually changed something
                  (or failed), launch Hermes to read logs and systemd state.
                  Updater run history (JSON + logs, append-only) lives in those
                  services' appdata (`updater/docker`, `updater/system`); last.json
                  is retargeted at the start of each run.
                  Broken: notify the Hermes home channel; for Docker, retag the previous image
                  and restart the containers. Warnings or migration hints: notify only, keep
                  the new image. Clean: no message. No-op updater runs skip Hermes entirely.
                  System updates are never rolled back automatically.
                  Notifications use `hermes send --to all` (every configured home channel),
                  then `hermes send --to telegram` if needed. The first `telegramAllowedUserId`
                  is set as TELEGRAM_HOME_CHANNEL.
                '';
              };

              stateDir = mkOption {
                type = types.str;
                internal = true;
                default = "${config.neo.core.volumes.appdata}/hermes";
                description = "State directory for Hermes data (HERMES_HOME inside)";
              };
            }
            // lib.neo.mkReverseProxyOptions {subdomain = "hermes";}
            // lib.neo.mkSystemdUnits [
              "hermes-dashboard"
              "hermes-agent"
            ]
            // lib.neo.mkAppdata config.neo.services.hermes.stateDir
            // lib.neo.mkServiceMeta {
              category = "AI";
              icon = "https://raw.githubusercontent.com/NousResearch/hermes-agent/main/website/static/img/logo.png";
              description = ''
                Hermes Agent is the self-improving AI agent built by Nous Research. The only agent with a built-in learning loop — it creates skills from experience, improves them during use, nudges itself to persist knowledge, and builds a deepening model of who you are across sessions.
                It is not a coding copilot tethered to an IDE or a chatbot wrapper around a single API. An autonomous agent that lives on your server, remembers what it learns, and gets more capable the longer it runs. Deploy it on a $5 VPS, a GPU cluster, or serverless infrastructure that costs nearly nothing when idle.
                Interact with it from Telegram, Discord, Slack, WhatsApp, Signal, Email, CLI, and its web dashboard. Features include persistent memory, autonomous skill creation and refinement, scheduled automations, parallel subagents, real sandboxing with multiple backends, full web and browser control, vision, and support for virtually any LLM provider.
              '';
              projectUrl = "https://hermes-agent.nousresearch.com/";
              githubUrl = "https://github.com/NousResearch/hermes-agent";
              releaseUrl = "https://github.com/NousResearch/hermes-agent/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Hermes Agent service configuration";
      };
    };
}
