# OpenClaw service options.
{...}: {
  flake.modules.nixos.openclaw-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.openclaw = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "OpenClaw service" {rank = 0;};

              gatewayPort = mkOption {
                type = types.port;
                default = 18789;
                description = "Port for the OpenClaw gateway";
              };
              gatewayToken = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Gateway authentication token";
                helper = lib.neo.helpers.randomToken;
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
                  At least one of anthropicApiKey or openaiApiKey must be set.
                '';
              };
              openaiApiKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = ''
                  OpenAI API key.
                  At least one of anthropicApiKey or openaiApiKey must be set.
                '';
              };
              xaiApiKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "xAI (Grok) API key";
              };

              defaultModel = mkOption {
                type = types.nullOr types.str;
                default = null;
                example = "xai/grok-3";
                description = ''
                  Default LLM model for the agent (e.g. "xai/grok-3",
                  "anthropic/claude-sonnet-4-20250514", "openai/gpt-4o").
                  When null, the gateway picks its own default.
                '';
              };

              documents = mkOption {
                type = types.nullOr types.path;
                default = null;
                description = ''
                  Path to the documents directory containing AGENTS.md, SOUL.md, TOOLS.md, etc.
                  These files configure the bot's personality and capabilities.
                '';
              };

              extraEnvironment = mkOption {
                type = types.attrsOf types.str;
                default = {};
                description = "Additional environment variables for the gateway process";
              };

              environmentFiles = mkOption {
                type = types.listOf types.str;
                default = [];
                description = ''
                  List of environment files to load into the gateway service.
                  Use this for secrets that should not be in the Nix store.
                  Files should contain KEY=VALUE pairs, one per line.
                '';
              };

              stateDir = mkOption {
                type = types.str;
                default = "/var/lib/openclaw";
                description = "State directory for OpenClaw data";
              };

              extraConfig = mkOption {
                type = types.attrs;
                default = {};
                description = ''
                  Extra OpenClaw JSON config attributes, deep-merged into the final config.
                  See the nix-openclaw documentation for all available options.
                '';
              };
            }
            // lib.neo.mkReverseProxyOptions {subdomain = "openclaw";}
            // lib.neo.mkAppdata config.neo.services.openclaw.stateDir
            // lib.neo.mkServiceMeta {
              category = "AI";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/openclaw.svg";
              description = ''
                OpenClaw is a self-hosted personal AI assistant and multi-channel gateway you run on your own devices.
                It bridges messaging platforms (Telegram, WhatsApp, Discord, Slack, Signal, iMessage, and 20+ more) to autonomous LLM agents with persistent memory, tool use, browser control, shell access, and extensible skills/plugins.
                Message your lobster (🦞) from any connected chat app and watch it take real actions on the host while keeping all data and execution private under your control.
                Supports Anthropic, OpenAI, xAI/Grok and local models; includes a web Control UI, companion apps, and mobile nodes for an always-on agent experience.
              '';
              projectUrl = "https://openclaw.ai/";
              githubUrl = "https://github.com/openclaw/openclaw";
              releaseUrl = "https://github.com/openclaw/openclaw/releases";
            };
        };
        default = {};
        description = "OpenClaw service configuration";
      };
    };
}
