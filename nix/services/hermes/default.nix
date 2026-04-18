# Hermes service implementation.
# Mirrors openclaw options for seamless migration
# Uses the official hermes-agent NixOS module for declarative systemd service.
#
# Full sudo access + no sandboxing (matches OpenClaw exactly):
# - hermes user in wheel + docker groups
# - passwordless sudo (NOPASSWD + SETENV) for ALL commands
# - All hardening disabled (ProtectSystem, NoNewPrivileges, etc.)
# - Full read/write access to the system, docker, systemctl, etc.
{inputs, ...}: {
  flake.modules.nixos.hermes = {
    config,
    pkgs,
    lib,
    ...
  }: let
    cfg = config.neo.services.hermes;

    baseSettings = {
      model = {
        default = cfg.defaultModel or "xai/grok-4.20-0309-reasoning";
        provider =
          if (cfg.xaiApiKey != null)
          then "xai"
          else if (cfg.anthropicApiKey != null)
          then "anthropic"
          else "openrouter";
      };
      telegram = {
        channel_prompts = cfg.telegramGroups or {};
      };
      toolsets = ["all"];
      terminal = {
        backend = "local";
        cwd = "${cfg.stateDir}/workspace"; # suppresses MESSAGING_CWD deprecation
      };
      api = {
        enabled = true;
        port = cfg.gatewayPort;
      };
    };

    hermesSettings = lib.recursiveUpdate baseSettings cfg.extraConfig;

    hermesEnv =
      lib.filterAttrs (_: v: v != null && v != "") {
        XAI_API_KEY = cfg.xaiApiKey;
        ANTHROPIC_API_KEY = cfg.anthropicApiKey;
        OPENAI_API_KEY = cfg.openaiApiKey;
        TELEGRAM_BOT_TOKEN = cfg.telegramBotToken;
        HERMES_GATEWAY_TOKEN = cfg.gatewayToken;
        TELEGRAM_ALLOWED_USERS = lib.concatStringsSep "," (map toString cfg.telegramAllowedUserId);
      }
      // cfg.extraEnvironment;
  in {
    config = lib.mkIf cfg.enabled (lib.mkMerge [
      {
        users.users.hermes = {
          extraGroups = ["wheel" "docker"];
          linger = true;
        };

        security.sudo.extraRules = [
          {
            users = ["hermes"];
            commands = [
              {
                command = "ALL";
                options = ["NOPASSWD" "SETENV"];
              }
            ];
          }
        ];

        services.hermes-agent = {
          enable = true;
          stateDir = cfg.stateDir;
          workingDirectory = "${cfg.stateDir}/workspace";
          addToSystemPackages = true;
          settings = hermesSettings;
          environment = hermesEnv;
          environmentFiles = cfg.environmentFiles;
          # Documents: map common ones. Expand as needed.
          documents =
            if (cfg.documents != null)
            then {
              "AGENTS.md" = "${cfg.documents}/AGENTS.md";
            }
            else {};
          restart = "always";
          restartSec = 5;
        };

        # Disable ALL sandboxing/hardening (full system access like OpenClaw)
        systemd.services.hermes-agent.serviceConfig = {
          ReadWritePaths = ["/"];
          NoNewPrivileges = lib.mkForce false;
          ProtectHome = lib.mkForce false;
          ProtectSystem = lib.mkForce false;
          PrivateTmp = lib.mkForce false;
          RestrictNamespaces = lib.mkForce false;
          LockPersonality = lib.mkForce false;
          MemoryDenyWriteExecute = lib.mkForce false;
          UMask = "0007";

          CapabilityBoundingSet = ["CAP_SETUID" "CAP_SETGID" "CAP_AUDIT_WRITE" "CAP_DAC_OVERRIDE" "CAP_SYS_ADMIN"];
          AmbientCapabilities = ["CAP_SETUID" "CAP_SETGID" "CAP_SYS_ADMIN"];
        };

        system.activationScripts.hermes-workspace = lib.neo.mkActivationScriptForDir config {
          dirPath = "${cfg.stateDir}/workspace";
          user = "hermes";
          group = "hermes";
          mode = "2770";
        };

        environment.variables.HERMES_HOME = "${cfg.stateDir}/.hermes";
      }

      (lib.mkIf (cfg.xaiApiKey == null && cfg.anthropicApiKey == null && cfg.openaiApiKey == null) {
        warnings = [
          "neo.services.hermes: At least one of xaiApiKey, anthropicApiKey or openaiApiKey should be set."
        ];
      })
    ]);
  };
}
