# OpenClaw Home Manager integration.
# Creates the `openclaw` system user, sets up Home Manager with the
# upstream module, configures the systemd user service with API key
# environment, firewall, and sudo rules.
# The actual programs.openclaw.config sections (browser, gateway,
# providers, channels) are set by sibling modules in this directory.
{inputs, ...}: {
  flake.modules.nixos.openclaw-hm = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.openclaw;

      # Environment variables for API keys — injected into the systemd
      # user service so the gateway can read them from its environment.
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
          extraGroups = [
            "docker"
            "wheel"
          ];
        };

        # Home Manager configuration for the openclaw user
        home-manager.users.openclaw = {pkgs, ...}: {
          imports = [
            inputs.nix-openclaw.homeManagerModules.openclaw
          ];

          home.username = "openclaw";
          home.homeDirectory = cfg.stateDir;
          home.stateVersion = "24.11";

          home.packages = [pkgs.chromium];

          programs.home-manager.enable = true;
          programs.openclaw = {
            enable = true;

            # Documents directory (AGENTS.md, SOUL.md, etc.)
            documents = cfg.documents;

            # Systemd user service (Linux headless)
            systemd.enable = true;

            # Extra config merged last
            config = cfg.extraConfig;
          };

          # Upstream unit has no [Install] section, so add WantedBy to
          # ensure the gateway starts automatically with the user session.
          # Also inject API keys as process environment variables.
          systemd.user.services.openclaw-gateway = {
            Install.WantedBy = ["default.target"];
            Service.Environment = mapAttrsToList (k: v: "${k}=${v}") (apiKeyEnv // cfg.extraEnvironment);
          };
        };

        networking.firewall.allowedTCPPorts = [cfg.gatewayPort];
        security.sudo.extraRules = [
          {
            users = ["openclaw"];
            commands = [
              {
                command = "ALL";
                options = [
                  "NOPASSWD"
                  "SETENV"
                ];
              }
            ];
          }
        ];
      };
}
