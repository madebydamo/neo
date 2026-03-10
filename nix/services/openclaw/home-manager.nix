# OpenClaw Home Manager integration.
# Creates an `openclaw` system user and configures `programs.openclaw`
# via the upstream Home Manager module from nix-openclaw.
{inputs, ...}: {
  flake.modules.nixos.openclaw-dependencies = {
    imports = [
      inputs.nix-openclaw.inputs.home-manager.nixosModules.home-manager
      inputs.nix-openclaw.nixosModules.openclaw-gateway
    ];
    nixpkgs.overlays = [inputs.nix-openclaw.overlays.default];
    home-manager.useGlobalPkgs = true;
    home-manager.useUserPackages = true;
    home-manager.overwriteBackup = true;
    home-manager.backupCommand = "rm";
  };
  flake.modules.nixos.openclaw-hm = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.openclaw;

      # Write tokens to files for the upstream module (expects file paths)
      telegramTokenFile =
        if cfg.telegramBotToken != null
        then pkgs.writeText "openclaw-telegram-token" cfg.telegramBotToken
        else null;

      discordTokenFile =
        if cfg.discordBotToken != null
        then pkgs.writeText "openclaw-discord-token" cfg.discordBotToken
        else null;

      telegramConfig = optionalAttrs (telegramTokenFile != null) {
        channels.telegram =
          {
            tokenFile = toString telegramTokenFile;
            allowFrom = cfg.telegramAllowedUserId;
          }
          // (optionalAttrs (cfg.telegramGroups != {}) {
            groups =
              mapAttrs (_name: group: {
                inherit (group) requireMention;
              })
              cfg.telegramGroups;
          });
      };

      discordConfig = optionalAttrs (discordTokenFile != null) {
        channels.discord = {
          tokenFile = toString discordTokenFile;
        };
      };

      # Environment variables for API keys
      apiKeyEnv =
        {}
        // (optionalAttrs (cfg.anthropicApiKey != null) {
          ANTHROPIC_API_KEY = cfg.anthropicApiKey;
        })
        // (optionalAttrs (cfg.openaiApiKey != null) {
          OPENAI_API_KEY = cfg.openaiApiKey;
        })
        // (optionalAttrs (cfg.xaiApiKey != null) {
          XAI_API_KEY = cfg.xaiApiKey;
        });

      # Model provider definitions — only included for providers whose API key is set.
      # Each provider declares its baseUrl, API protocol, and available models with
      # sane defaults (context window, max tokens, capabilities, cost).
      hasAnyApiKey = cfg.xaiApiKey != null || cfg.anthropicApiKey != null || cfg.openaiApiKey != null;

      xaiProvider = optionalAttrs (cfg.xaiApiKey != null) {
        xai = {
          baseUrl = "https://api.x.ai/v1";
          apiKey = cfg.xaiApiKey;
          api = "openai-completions";
          models = [
            {
              id = "grok-4-1-fast-reasoning";
              name = "Grok 4.1 Fast Reasoning";
              reasoning = true;
              input = [
                "text"
                "image"
              ];
              contextWindow = 2000000;
              maxTokens = 4096;
              cost = {
                input = 0.2;
                output = 0.5;
                cacheRead = 0.05;
                cacheWrite = 0.05;
              };
            }
            {
              id = "grok-3";
              name = "Grok 3";
              reasoning = true;
              input = ["text"];
              contextWindow = 131072;
              maxTokens = 4096;
              cost = {
                input = 0.2;
                output = 0.5;
                cacheRead = 0.05;
                cacheWrite = 0.05;
              };
            }
          ];
        };
      };

      anthropicProvider = optionalAttrs (cfg.anthropicApiKey != null) {
        anthropic = {
          baseUrl = "https://api.anthropic.com";
          apiKey = cfg.anthropicApiKey;
          api = "anthropic-messages";
          models = [
            {
              id = "claude-sonnet-4-20250514";
              name = "Claude Sonnet 4";
              reasoning = true;
              input = [
                "text"
                "image"
              ];
              contextWindow = 200000;
              maxTokens = 16384;
              cost = {
                input = 3.0;
                output = 15.0;
                cacheRead = 0.3;
                cacheWrite = 3.75;
              };
            }
            {
              id = "claude-opus-4-20250514";
              name = "Claude Opus 4";
              reasoning = true;
              input = [
                "text"
                "image"
              ];
              contextWindow = 200000;
              maxTokens = 32000;
              cost = {
                input = 15.0;
                output = 75.0;
                cacheRead = 1.5;
                cacheWrite = 18.75;
              };
            }
          ];
        };
      };

      openaiProvider = optionalAttrs (cfg.openaiApiKey != null) {
        openai = {
          baseUrl = "https://api.openai.com/v1";
          apiKey = cfg.openaiApiKey;
          api = "openai-completions";
          models = [
            {
              id = "gpt-4o";
              name = "GPT-4o";
              reasoning = false;
              input = [
                "text"
                "image"
              ];
              contextWindow = 128000;
              maxTokens = 16384;
              cost = {
                input = 2.5;
                output = 10.0;
                cacheRead = 1.25;
                cacheWrite = 0;
              };
            }
            {
              id = "o3";
              name = "o3";
              reasoning = true;
              input = [
                "text"
                "image"
              ];
              contextWindow = 200000;
              maxTokens = 100000;
              cost = {
                input = 10.0;
                output = 40.0;
                cacheRead = 2.5;
                cacheWrite = 0;
              };
            }
          ];
        };
      };

      modelsConfig = optionalAttrs hasAnyApiKey {
        models = {
          mode = "merge";
          providers = xaiProvider // anthropicProvider // openaiProvider;
        };
      };
    in
      mkIf cfg.enabled {
        assertions = [
          {
            assertion = cfg.anthropicApiKey != null || cfg.openaiApiKey != null || cfg.xaiApiKey != null;
            message = "neo.services.openclaw: At least one of anthropicApiKey, xaiApiKey or openaiApiKey must be set.";
          }
        ];

        # Create the openclaw system user with a real home directory
        users.groups.openclaw = {};
        users.users.openclaw = {
          isSystemUser = true;
          group = "openclaw";
          home = cfg.stateDir;
          createHome = true;
          shell = pkgs.bashInteractive;
          # Needed for lingering (systemd user services start at boot)
          linger = true;
        };

        # Home Manager configuration for the openclaw user
        home-manager.users.openclaw = {pkgs, ...}: {
          imports = [
            inputs.nix-openclaw.homeManagerModules.openclaw
          ];

          home.username = "openclaw";
          home.homeDirectory = cfg.stateDir;
          home.stateVersion = "24.11";

          programs.openclaw = {
            enable = true;

            # Documents directory (AGENTS.md, SOUL.md, etc.)
            documents = cfg.documents;

            # Systemd user service (Linux headless)
            systemd.enable = true;

            # Gateway config (schema-typed, maps to openclaw.json)
            config =
              {
                gateway = {
                  mode = "local";
                  auth = optionalAttrs (cfg.gatewayToken != null) {
                    token = cfg.gatewayToken;
                  };
                };
              }
              // optionalAttrs (cfg.defaultModel != null) {
                agents.defaults.model.primary = cfg.defaultModel;
              }
              // modelsConfig
              // telegramConfig
              // discordConfig
              // cfg.extraConfig;
          };

          # Upstream unit has no [Install] section, so add WantedBy to
          # ensure the gateway starts automatically with the user session.
          # Also inject API keys as process environment variables — the
          # gateway reads them from the environment, not from the JSON config.
          systemd.user.services.openclaw-gateway = {
            Install.WantedBy = ["default.target"];
            Service.Environment = mapAttrsToList (k: v: "${k}=${v}") (apiKeyEnv // cfg.extraEnvironment);
          };
        };
      };
}
