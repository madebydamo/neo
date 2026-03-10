# OpenClaw service options.
{...}: {
  flake.modules.nixos.openclaw-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.openclaw = mkOption {
        type = types.submodule {
          options = {
            enabled = mkEnableOption (lib.mdDoc "OpenClaw service");

            gatewayPort = mkOption {
              type = types.port;
              default = 18789;
              description = lib.mdDoc "Port for the OpenClaw gateway";
            };
            gatewayToken = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = lib.mdDoc "Gateway authentication token";
            };

            subdomain = mkOption {
              type = types.nullOr types.str;
              default = "openclaw";
              description = lib.mdDoc "Subdomain for the service (used by swag reverse proxy)";
            };
            proxyConf = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = lib.mdDoc "Nginx proxy conf for swag";
            };

            telegramBotToken = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = lib.mdDoc ''
                Telegram bot token string.
                Create a bot via @BotFather on Telegram.
              '';
            };
            telegramAllowedUserId = mkOption {
              type = types.listOf types.int;
              default = [];
              description = lib.mdDoc ''
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
                      description = lib.mdDoc "Whether the bot requires an @mention in this group";
                    };
                  };
                }
              );
              default = {};
              description = lib.mdDoc ''
                Per-group Telegram settings. Keys are chat IDs (as strings) or "*" for default.
              '';
            };

            anthropicApiKey = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = lib.mdDoc ''
                Anthropic (Claude) API key.
                At least one of anthropicApiKey or openaiApiKey must be set.
              '';
            };
            openaiApiKey = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = lib.mdDoc ''
                OpenAI API key.
                At least one of anthropicApiKey or openaiApiKey must be set.
              '';
            };
            xaiApiKey = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = lib.mdDoc "xAI (Grok) API key";
            };

            defaultModel = mkOption {
              type = types.nullOr types.str;
              default = null;
              example = "xai/grok-3";
              description = lib.mdDoc ''
                Default LLM model for the agent (e.g. "xai/grok-3",
                "anthropic/claude-sonnet-4-20250514", "openai/gpt-4o").
                When null, the gateway picks its own default.
              '';
            };

            discordBotToken = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = lib.mdDoc "Discord bot token (optional)";
            };

            documents = mkOption {
              type = types.nullOr types.path;
              default = null;
              description = lib.mdDoc ''
                Path to the documents directory containing AGENTS.md, SOUL.md, TOOLS.md, etc.
                These files configure the bot's personality and capabilities.
              '';
            };

            extraEnvironment = mkOption {
              type = types.attrsOf types.str;
              default = {};
              description = lib.mdDoc "Additional environment variables for the gateway process";
            };

            environmentFiles = mkOption {
              type = types.listOf types.str;
              default = [];
              description = lib.mdDoc ''
                List of environment files to load into the gateway service.
                Use this for secrets that should not be in the Nix store.
                Files should contain KEY=VALUE pairs, one per line.
              '';
            };

            stateDir = mkOption {
              type = types.str;
              default = "/var/lib/openclaw";
              description = lib.mdDoc "State directory for OpenClaw data";
            };

            extraConfig = mkOption {
              type = types.attrs;
              default = {};
              description = lib.mdDoc ''
                Extra OpenClaw JSON config attributes, deep-merged into the final config.
                See the nix-openclaw documentation for all available options.
              '';
            };
          };
        };
        default = {};
        description = lib.mdDoc "OpenClaw service configuration";
      };
    };
}
