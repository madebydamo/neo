{
  config,
  lib,
  ...
}:
with lib; {
  options.neo.services.openclaw = mkOption {
    type = types.submodule {
      options = {
        enabled = mkEnableOption (lib.mdDoc "OpenClaw service");

        # Gateway configuration
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

        # Reverse proxy
        subdomain = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc "Subdomain for the service (used by swag reverse proxy)";
        };
        proxyConf = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc "Nginx proxy conf for swag";
        };

        # Telegram bot configuration
        telegramBotTokenFile = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc ''
            Path to a file containing the Telegram bot token.
            Create a bot via @BotFather on Telegram.
            The file should contain only the token string.
          '';
        };
        telegramAllowFrom = mkOption {
          type = types.listOf types.int;
          default = [];
          description = lib.mdDoc ''
            List of Telegram user/chat IDs allowed to interact with the bot.
            Get your ID from @userinfobot on Telegram.
            Use negative IDs for group chats.
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

        # Provider API keys
        anthropicApiKeyFile = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc ''
            Path to a file containing the Anthropic API key.
            Used for Claude AI integration.
          '';
        };
        openaiApiKeyFile = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc ''
            Path to a file containing the OpenAI API key (optional).
          '';
        };

        # Discord (optional alternative to Telegram)
        discordBotTokenFile = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc ''
            Path to a file containing the Discord bot token (optional).
          '';
        };

        # Documents directory
        documents = mkOption {
          type = types.nullOr types.path;
          default = null;
          description = lib.mdDoc ''
            Path to the documents directory containing AGENTS.md, SOUL.md, TOOLS.md, etc.
            These files configure the bot's personality and capabilities.
          '';
        };

        # Extra environment variables
        extraEnvironment = mkOption {
          type = types.attrsOf types.str;
          default = {};
          description = lib.mdDoc "Additional environment variables for the gateway process";
        };

        # Extra environment files (for secrets)
        environmentFiles = mkOption {
          type = types.listOf types.str;
          default = [];
          description = lib.mdDoc ''
            List of environment files to load into the gateway service.
            Use this for secrets that should not be in the Nix store.
            Files should contain KEY=VALUE pairs, one per line.
          '';
        };

        # State directory
        stateDir = mkOption {
          type = types.str;
          default = "/var/lib/openclaw";
          description = lib.mdDoc "State directory for OpenClaw data";
        };

        # Extra config (deep-merged into the OpenClaw JSON config)
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
}
