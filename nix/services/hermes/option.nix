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
                default = "${config.neo.volumes.appdata}/hermes";
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
            // lib.neo.mkReverseProxyOptions {subdomain = "hermes";};
        };
        default = {};
        description = "Hermes Agent service configuration";
      };
    };
}
