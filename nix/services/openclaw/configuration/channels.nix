# OpenClaw messaging channel configuration.
# Handles Telegram and Discord bot token files and channel settings.
{...}: {
  flake.modules.nixos.openclaw-config-channels = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.openclaw;

      telegramTokenFile =
        if cfg.telegramBotToken != null
        then pkgs.writeText "openclaw-telegram-token" cfg.telegramBotToken
        else null;

      discordTokenFile =
        if cfg.discordBotToken != null
        then pkgs.writeText "openclaw-discord-token" cfg.discordBotToken
        else null;

      telegramConfig = optionalAttrs (telegramTokenFile != null) {
        telegram =
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
        discord = {
          tokenFile = toString discordTokenFile;
        };
      };

      channelsConfig = telegramConfig // discordConfig;
    in
      mkIf (cfg.enabled && channelsConfig != {}) {
        home-manager.users.openclaw.programs.openclaw.config.channels = channelsConfig;
      };
}
