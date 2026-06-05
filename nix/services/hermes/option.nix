# Hermes service options. Mirrors openclaw options for easy migration
# by renaming [services.openclaw] to [services.hermes] in settings.toml.
{...}: {
  flake.modules.nixos.hermes-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.hermes = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Hermes Agent service (replaces OpenClaw)";

              gatewayPort = mkOption {
                type = types.port;
                default = 18789;
                description = "Port for the Hermes gateway/API";
              };

              dashboardPort = mkOption {
                type = types.port;
                default = 9119;
                description = "Port for the Hermes web dashboard UI (uses tinyauth via SWAG)";
              };

              gatewayToken = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Gateway authentication token";
              };

              telegramBotToken = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = ''
                  Telegram bot token string.
                  Create a bot via @BotFather on Telegram.
                '';
              };

              telegramAllowedUserId = mkOption {
                type = types.listOf types.int;
                default = [];
                description = ''
                  List of Telegram user/chat IDs allowed to interact with the bot.
                  Get your ID from @userinfobot on Telegram.
                '';
              };

              telegramGroups = mkOption {
                type = types.attrsOf (
                  types.submodule {
                    options = {
                      requireMention = mkOption {
                        type = types.bool;
                        default = true;
                        description = "Whether the bot requires an @mention in this group";
                      };
                    };
                  }
                );
                default = {};
                description = ''
                  Per-group Telegram settings. Keys are chat IDs (as strings) or "*" for default.
                '';
              };

              anthropicApiKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = ''
                  Anthropic (Claude) API key.
                '';
              };

              openaiApiKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = ''
                  OpenAI API key.
                '';
              };

              xaiApiKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "xAI (Grok) API key";
              };

              defaultModel = mkOption {
                type = types.nullOr types.str;
                default = "grok-4.3";
                description = ''
                  Default LLM model for the agent (e.g. "grok-4.20-0309-reasoning",
                  "claude-sonnet-4", "gpt-4o").
                '';
              };

              documents = mkOption {
                type = types.nullOr types.path;
                default = null;
                description = ''
                  Path to the documents directory containing AGENTS.md, SOUL.md, etc.
                  These files configure the bot's personality and capabilities.
                  For Hermes, files are copied to workingDirectory.
                '';
              };

              extraEnvironment = mkOption {
                type = types.attrsOf types.str;
                default = {};
                description = "Additional environment variables for the hermes service";
              };

              environmentFiles = mkOption {
                type = types.listOf types.str;
                default = [];
                description = ''
                  List of environment files to load into the service.
                  Use this for secrets that should not be in the Nix store.
                '';
              };

              stateDir = mkOption {
                type = types.str;
                default = "${config.neo.core.volumes.appdata}/hermes";
                description = "State directory for Hermes data (HERMES_HOME inside)";
              };

              extraConfig = mkOption {
                type = types.attrs;
                default = {};
                description = ''
                  Extra Hermes config attributes, deep-merged into settings.
                  See Hermes docs for available options.
                '';
              };
            }
            // lib.neo.mkReverseProxyOptions {subdomain = "hermes";}
            // lib.neo.mkServiceMeta {
              icon = "https://raw.githubusercontent.com/NousResearch/hermes-agent/main/website/static/img/logo.png";
              description = ''
                Hermes Agent is the self-improving AI agent built by Nous Research. The only agent with a built-in learning loop — it creates skills from experience, improves them during use, nudges itself to persist knowledge, and builds a deepening model of who you are across sessions.
                It is not a coding copilot tethered to an IDE or a chatbot wrapper around a single API. An autonomous agent that lives on your server, remembers what it learns, and gets more capable the longer it runs. Deploy it on a $5 VPS, a GPU cluster, or serverless infrastructure that costs nearly nothing when idle.
                Interact with it from Telegram, Discord, Slack, WhatsApp, Signal, Email, CLI, and its web dashboard. Features include persistent memory, autonomous skill creation and refinement, scheduled automations, parallel subagents, real sandboxing with multiple backends, full web and browser control, vision, and support for virtually any LLM provider.
              '';
              projectUrl = "https://hermes-agent.nousresearch.com/";
              githubUrl = "https://github.com/NousResearch/hermes-agent";
              releaseUrl = "https://github.com/NousResearch/hermes-agent/releases";
            };
        };
        default = {};
        description = "Hermes Agent service configuration";
      };
    };
}
