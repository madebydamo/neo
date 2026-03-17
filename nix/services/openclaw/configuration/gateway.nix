# OpenClaw gateway, tools, and agent defaults configuration.
# Handles gateway mode/auth/bind, elevated tools, and default model selection.
{...}: {
  flake.modules.nixos.openclaw-config-gateway = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.openclaw;
      domain = config.neo.services.swag.domain;
    in
      mkIf cfg.enabled {
        home-manager.users.openclaw.programs.openclaw.config =
          {
            gateway = {
              mode = "local";
              auth = optionalAttrs (cfg.gatewayToken != null) {
                mode = "token";
                token = cfg.gatewayToken;
              };
              bind = "lan";
              controlUi.allowedOrigins = ["https://${cfg.subdomain}.${domain}"];
            };
          }
          // optionalAttrs (cfg.defaultModel != null) {
            agents.defaults.model.primary = cfg.defaultModel;
          };
      };
}
