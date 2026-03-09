# OpenClaw service implementation.
{...}: {
  flake.modules.nixos.openclaw = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.openclaw;

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

      discordConfig = lib.optionalAttrs (cfg.discordBotTokenFile != null) {
        channels.discord = {
          tokenFile = cfg.discordBotTokenFile;
        };
      };

      envFiles =
        cfg.environmentFiles
        ++ (lib.optional (cfg.anthropicApiKeyFile != null) cfg.anthropicApiKeyFile)
        ++ (lib.optional (cfg.openaiApiKeyFile != null) cfg.openaiApiKeyFile);
    in
      mkIf cfg.enabled {
        services.openclaw-gateway = {
          enable = true;
          port = cfg.gatewayPort;
          stateDir = cfg.stateDir;

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

          environmentFiles = cfg.environmentFiles;
        };
      };
}
