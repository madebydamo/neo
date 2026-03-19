# OpenClaw messaging channel configuration.
# Handles Telegram bot token files and channel settings.
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

      telegramConfig = optionalAttrs (telegramTokenFile != null) {
        telegram =
          {
            tokenFile = toString telegramTokenFile;
            allowFrom = cfg.telegramAllowedUserId;
            execApprovals = {
              enabled = true;
              approvers = lists.take 1 cfg.telegramAllowedUserId;
            };
          }
          // (optionalAttrs (cfg.telegramGroups != {}) {
            groups =
              mapAttrs (_name: group: {
                inherit (group) requireMention;
                groupPolicy = "open";
              })
              cfg.telegramGroups;
          });
      };
    in
      mkIf (cfg.enabled && telegramConfig != {}) {
        home-manager.users.openclaw.programs.openclaw.config.channels = telegramConfig;
      };
}
