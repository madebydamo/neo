# OpenClaw gateway, tools, and agent defaults configuration.
# Handles gateway mode/auth/bind, elevated tools, and default model selection.
{...}: {
  flake.modules.nixos.openclaw-config-tools = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.openclaw;
    in
      mkIf cfg.enabled {
        home-manager.users.openclaw.programs.openclaw.config = {
          tools = {
            elevated = {
              enabled = true;
              allowFrom.telegram = cfg.telegramAllowedUserId;
            };
            exec = {
              host = "gateway";
              security = "full";
              ask = "on-miss";
            };
            web.search = optionalAttrs (cfg.xaiApiKey != null) {
              grok = {
                apiKey = cfg.xaiApiKey;
                model = "xai/grok-4-1-fast-reasoning";
              };
              enabled = true;
              provider = "grok";
            };
          };
        };
      };
}
