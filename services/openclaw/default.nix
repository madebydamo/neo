{
  config,
  lib,
  pkgs,
  ...
}:
with lib; let
  cfg = config.neo.services.openclaw;

  # Build the Telegram channel config
  telegramConfig = lib.optionalAttrs (cfg.telegramBotTokenFile != null) {
    channels.telegram =
      {
        tokenFile = cfg.telegramBotTokenFile;
        allowFrom = cfg.telegramAllowFrom;
      }
      // (lib.optionalAttrs (cfg.telegramGroups != {}) {
        groups =
          lib.mapAttrs (_name: group: {
            inherit (group) requireMention;
          })
          cfg.telegramGroups;
      });
  };

  # Build the Discord channel config
  discordConfig = lib.optionalAttrs (cfg.discordBotTokenFile != null) {
    channels.discord = {
      tokenFile = cfg.discordBotTokenFile;
    };
  };

  # Collect environment files for secrets
  envFiles =
    cfg.environmentFiles
    ++ (lib.optional (cfg.anthropicApiKeyFile != null) cfg.anthropicApiKeyFile)
    ++ (lib.optional (cfg.openaiApiKeyFile != null) cfg.openaiApiKeyFile);
in
  {
    imports = [
      ./option.nix
      ./swag.nix
    ];
  }
  // (mkIf cfg.enabled {
    # Configure the upstream nix-openclaw NixOS module
    services.openclaw-gateway = {
      enable = true;
      port = cfg.gatewayPort;
      stateDir = cfg.stateDir;

      # Deep-merged JSON config for OpenClaw
      config =
        {
          gateway = {
            mode = "local";
            auth = lib.optionalAttrs (cfg.gatewayToken != null) {
              token = cfg.gatewayToken;
            };
          };
        }
        // telegramConfig
        // discordConfig
        // cfg.extraConfig;

      # Environment variables
      environment =
        {
          TZ = "Europe/Zurich";
        }
        // (lib.optionalAttrs (cfg.anthropicApiKeyFile != null) {
          ANTHROPIC_API_KEY_FILE = cfg.anthropicApiKeyFile;
        })
        // (lib.optionalAttrs (cfg.openaiApiKeyFile != null) {
          OPENAI_API_KEY_FILE = cfg.openaiApiKeyFile;
        })
        // cfg.extraEnvironment;

      # Environment files for loading secrets
      environmentFiles = cfg.environmentFiles;
    };
  })
