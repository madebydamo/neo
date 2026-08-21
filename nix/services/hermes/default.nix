# Hermes service implementation.
# Uses the official hermes-agent NixOS module for declarative systemd service.
#
# Full sudo access + no sandboxing:
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

    # Only pin model.* when the operator set them or an API key implies a provider.
    # Omitting keys leaves config.yaml free for OAuth / prior values (Nix merge preserves
    # user keys it does not declare). Hermes managed mode still blocks dashboard saves.
    nonEmpty = v: v != null && v != "";

    derivedProvider =
      if nonEmpty cfg.modelProvider
      then cfg.modelProvider
      else if nonEmpty cfg.xaiApiKey
      then "xai"
      else if nonEmpty cfg.anthropicApiKey
      then "anthropic"
      else if nonEmpty cfg.openaiApiKey
      then "openai"
      else null;

    modelSection =
      (lib.optionalAttrs (nonEmpty cfg.defaultModel) {
        default = cfg.defaultModel;
      })
      // (lib.optionalAttrs (derivedProvider != null) {
        provider = derivedProvider;
      });

    hermesSettings =
      {
        telegram = {
          channel_prompts = cfg.telegramGroups or {};
        };
        toolsets = ["all"];
        terminal = {
          backend = "local";
          cwd = "${cfg.stateDir}/workspace";
        };
        api = {
          enabled = true;
          port = cfg.gatewayPort;
        };
        dashboard = {
          theme = "default";
        };
      }
      // lib.optionalAttrs (modelSection != {}) {
        model = modelSection;
      };

    # Fixed internal username — SWAG auto-login posts this; operators never type it.
    dashboardAuthUsername = "neo";

    dashboardPasswordSet =
      cfg.dashboardPassword != null && cfg.dashboardPassword != "";

    # CLI tools useful for Hermes (and operators) when the agent is installed.
    # The hermes wrapper suffixes node/ffmpeg/rg onto the *binary* PATH only.
    # Terminal, cron, skills, and login shells rebuild PATH from NixOS profiles
    # (system + hermes user), so these must live in extraPackages/systemPackages.
    agentCliTools = with pkgs; [
      # languages / toolchains (ad-hoc packages: uv venv or pip install --user)
      python3
      python3Packages.pip
      uv
      nodejs # node, npm, npx
      # media / documents
      ffmpeg
      imagemagick
      poppler-utils # pdftoppm, pdftotext, pdfinfo
      tesseract
      # structured data
      jq
      yq-go
      gron
      jo
      htmlq
      # search / filesystem
      ripgrep
      fd
      tree
      file
      eza
      bat
      # archives / transfer
      unzip
      zip
      p7zip
      rsync
      wget
      # system introspection
      lsof
      procps
      psmisc # pstree, killall, fuser
      openssl
      socat
      # text / scripting helpers
      moreutils
      gawk
      gnused
      sd
      parallel
      sqlite
      # ops
      gh
      docker-compose
    ];

    hermesEnv =
      lib.filterAttrs (_: v: v != null && v != "") {
        XAI_API_KEY = cfg.xaiApiKey;
        ANTHROPIC_API_KEY = cfg.anthropicApiKey;
        OPENAI_API_KEY = cfg.openaiApiKey;
        TELEGRAM_BOT_TOKEN = cfg.telegramBotToken;
        HERMES_GATEWAY_TOKEN = cfg.gatewayToken;
        TELEGRAM_ALLOWED_USERS = lib.concatStringsSep "," (map toString cfg.telegramAllowedUserId);
        GATEWAY_HEALTH_URL = "http://127.0.0.1:${toString cfg.gatewayPort}";
      }
      // lib.optionalAttrs dashboardPasswordSet {
        # Hermes 0.17+ refuses non-loopback binds without an auth provider.
        # SWAG intercepts /login and auto-posts these so only tinyauth is user-facing.
        HERMES_DASHBOARD_BASIC_AUTH_USERNAME = dashboardAuthUsername;
        HERMES_DASHBOARD_BASIC_AUTH_PASSWORD = cfg.dashboardPassword;
        HERMES_DASHBOARD_BASIC_AUTH_SECRET = cfg.dashboardPassword;
      };

    # Credentials JSON for the SWAG auto-login page (safe JS embedding via toJSON).
    autologinCredsJson = builtins.toJSON {
      provider = "basic";
      username = dashboardAuthUsername;
      password = cfg.dashboardPassword or "";
    };

    autologinHtml = ''
      <!DOCTYPE html>
      <html lang="en">
      <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <title>Hermes</title>
        <style>
          body { font-family: system-ui, sans-serif; display: grid; place-items: center;
                 min-height: 100vh; margin: 0; background: #0b0f14; color: #e6edf3; }
          p { opacity: 0.85; }
        </style>
      </head>
      <body>
        <p id="status">Signing in…</p>
        <script>
          (async () => {
            const params = new URLSearchParams(location.search);
            const next = params.get("next") || "/";
            const body = ${autologinCredsJson};
            body.next = next;
            try {
              const res = await fetch("/auth/password-login", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                credentials: "same-origin",
                body: JSON.stringify(body),
              });
              if (!res.ok) throw new Error("HTTP " + res.status);
              const data = await res.json();
              location.replace(data.next || next || "/");
            } catch (e) {
              document.getElementById("status").textContent =
                "Auto-login failed. Set neo.services.hermes.dashboardPassword and rebuild.";
            }
          })();
        </script>
      </body>
      </html>
    '';
  in {
    config = lib.mkIf cfg.enabled {
      assertions = [
        {
          assertion = dashboardPasswordSet;
          message = ''
            neo.services.hermes.dashboardPassword must be set (use the Generate helper in the UI).
            Hermes 0.17+ requires an auth provider for non-loopback dashboard binds; neo uses this
            password for internal basic auth and SWAG auto-login so only tinyauth is user-facing.
          '';
        }
      ];

      users.users.hermes = {
        extraGroups = ["wheel" "docker"];
        linger = true;
      };

      # Agent needs unrestricted host control (docker, systemctl, package tools, …).
      security.sudo.extraRules = lib.neo.mkSudoExtraRules {
        users = ["hermes"];
        all = true;
      };

      # Agent-oriented CLIs system-wide when Hermes is on (jq, fd, yq, …).
      environment.systemPackages = agentCliTools;

      services.hermes-agent = {
        enable = true;
        stateDir = cfg.stateDir;
        workingDirectory = "${cfg.stateDir}/workspace";
        addToSystemPackages = true;
        # Also on hermes user profile + gateway systemd PATH (see hermes-agent module).
        extraPackages = agentCliTools;
        settings = hermesSettings;
        extraDependencyGroups = ["all" "messaging" "homeassistant" "youtube" "voice"];
        environment = hermesEnv;
        # Workspace AGENTS.md + skill tree: hermes/skills.nix.
        restart = "always";
        restartSec = 5;
      };

      # Disable ALL sandboxing/hardening (full system access)
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

      system.activationScripts.hermes-workspace = lib.neo.mkEnsureDirs config [
        {
          dirPath = "${cfg.stateDir}/workspace";
          user = "hermes";
          group = "hermes";
          mode = "2770";
        }
      ];

      # SWAG serves this at /login so Hermes form-auth is invisible behind tinyauth.
      system.activationScripts.hermes-dashboard-autologin = lib.neo.mkActivationScriptForFile config {
        filePath = "${config.neo.core.volumes.appdata}/swag/www/hermes-autologin.html";
        content = autologinHtml;
        mode = "0644";
      };

      environment.variables.HERMES_HOME = "${cfg.stateDir}/.hermes";

      # Web dashboard — non-loopback bind requires Hermes basic_auth (tinyauth is edge auth via SWAG)
      systemd.services.hermes-dashboard = {
        description = "Hermes Agent Web Dashboard";
        wantedBy = ["multi-user.target"];
        after = ["hermes-agent.service" "network-online.target"];
        wants = ["network-online.target"];
        requires = ["hermes-agent.service"];

        serviceConfig = {
          User = "hermes";
          Group = "hermes";
          WorkingDirectory = cfg.stateDir;
          ExecStart = let
            pkg = config.services.hermes-agent.package;
          in "${pkg}/bin/hermes dashboard --host 0.0.0.0 --port ${toString cfg.dashboardPort} --no-open";
          Restart = "always";
          RestartSec = 5;

          # Relaxed security matching the gateway (dashboard manages configs, API keys, sessions)
          ReadWritePaths = ["/"];
          NoNewPrivileges = lib.mkForce false;
          ProtectHome = lib.mkForce false;
          ProtectSystem = lib.mkForce false;
          PrivateTmp = lib.mkForce false;
        };

        environment = hermesEnv;
        # Match the gateway unit: dashboard PTYs/skills inherit this PATH too.
        path = agentCliTools;
      };
    };
  };
}
