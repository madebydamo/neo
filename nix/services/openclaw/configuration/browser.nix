# OpenClaw headless browser configuration.
# Configures Chromium for headless browsing within the gateway.
{...}: {
  flake.modules.nixos.openclaw-config-browser = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.openclaw;
    in
      mkIf cfg.enabled {
        home-manager.users.openclaw.programs.openclaw.config.browser = {
          enabled = true;
          executablePath = "/etc/profiles/per-user/openclaw/bin/chromium-browser";
          headless = true;
          noSandbox = true;
        };
      };
}
