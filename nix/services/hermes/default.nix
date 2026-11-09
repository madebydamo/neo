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
    neoCfg = config.neo.services.neo;
    # Chat in the neo web UI: Hermes' API server (needs API_SERVER_KEY, ≥16
    # chars, from the gateway token) on 127.0.0.1:gatewayPort, used by neo-web.
    chatOn = cfg.gatewayToken != null && builtins.stringLength cfg.gatewayToken >= 16 && (neoCfg.enabled or false);
    # Settings tools (neo web /internal/mcp): only with a token and neo web on.
    neoToolsOn = cfg.neoToolsToken != null && cfg.neoToolsToken != "" && (neoCfg.enabled or false);
    neoHermesAuth = lib.neo.mkNeoHermesAuth pkgs ./oauth.py;

    # Only pin model.* when the operator set them. Omitting keys leaves config.yaml
    # free for OAuth / prior values (Nix merge preserves user keys it does not declare).
    # Hermes managed mode still blocks dashboard saves.
    llm = cfg.llm;

    modelSection =
      (lib.optionalAttrs (lib.neo.isSet llm.model) {
        default = llm.model;
      })
      // (lib.optionalAttrs (lib.neo.isSet llm.provider) {
        provider = llm.provider;
      })
      // (lib.optionalAttrs (lib.neo.isSet llm.baseUrl) {
        base_url = llm.baseUrl;
      })
      # The key itself comes from the env (lib.neo.mkHermesLlmEnv → hermes.env).
      # config.yaml is deep-merged, so an empty api_key clears an inline key an
      # older generation wrote there; an empty key counts as unset upstream.
      // (lib.optionalAttrs (lib.neo.isSet llm.provider && llm.provider == "custom" && lib.neo.isSet llm.apiKey) {
        key_env = lib.neo.hermesCustomKeyEnv;
        api_key = "";
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
      // lib.optionalAttrs neoToolsOn {
        # Hermes expands ''${VAR} in MCP headers from its env (hermes.env).
        mcp_servers.neo = {
          url = "http://127.0.0.1:${toString neoCfg.port}/internal/mcp";
          headers.Authorization = "Bearer \${NEO_AGENT_TOKEN}";
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

    # First allowed ID is the Telegram home channel (`hermes send --to telegram`).
    telegramHomeChannel =
      if cfg.telegramAllowedUserId == []
      then null
      else toString (builtins.head cfg.telegramAllowedUserId);

    hermesEnv =
      lib.filterAttrs (_: v: v != null && v != "") {
        TELEGRAM_ALLOWED_USERS = lib.concatStringsSep "," (map toString cfg.telegramAllowedUserId);
        TELEGRAM_HOME_CHANNEL = telegramHomeChannel;
        GATEWAY_HEALTH_URL = "http://127.0.0.1:${toString cfg.gatewayPort}";
        API_SERVER_HOST = "127.0.0.1";
        API_SERVER_PORT = toString cfg.gatewayPort;
      }
      // lib.optionalAttrs dashboardPasswordSet {
        # Hermes 0.17+ refuses non-loopback binds without an auth provider.
        # SWAG intercepts /login and auto-posts these so only tinyauth is user-facing.
        HERMES_DASHBOARD_BASIC_AUTH_USERNAME = dashboardAuthUsername;
      };

    # Secret half of the environment: rendered at activation (hermesEnvFile),
    # never in the store.
    hermesSecretEnv =
      lib.filterAttrs (_: v: v != null && v != "") {
        TELEGRAM_BOT_TOKEN = cfg.telegramBotToken;
        HERMES_GATEWAY_TOKEN = cfg.gatewayToken;
        # Turns on the API server the neo web chat talks to.
        API_SERVER_KEY = cfg.gatewayToken;
        NEO_AGENT_TOKEN =
          if neoToolsOn
          then cfg.neoToolsToken
          else null;
        # gh CLI; git itself gets the token from the machine git identity.
        GH_TOKEN = config.neo.core.git.tokens."github.com" or null;
      }
      // lib.neo.mkHermesLlmEnv {
        inherit (llm) provider apiKey;
      }
      // lib.optionalAttrs dashboardPasswordSet {
        HERMES_DASHBOARD_BASIC_AUTH_PASSWORD = cfg.dashboardPassword;
        HERMES_DASHBOARD_BASIC_AUTH_SECRET = cfg.dashboardPassword;
      };
    hermesEnvFile = config.neo.core.secrets.templates."hermes.env".path;

    # Credentials for the SWAG auto-login page. The password is a secret: a JSON
    # string literal rendered at activation (lib.neo.secrets.jsonString).
    autologinCredsJson = "{\"provider\": \"basic\", \"username\": ${builtins.toJSON dashboardAuthUsername}, \"password\": ${lib.neo.secrets.jsonString (cfg.dashboardPassword or "")}}";

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

      # The gateway reads it via $HERMES_HOME/.env and, as before, is not
      # restarted on a change (it may be driving the very switch).
      # The same token for neo web, which checks it on /internal/mcp.
      neo.core.secrets.templates."neo-agent-token" = lib.mkIf neoToolsOn {
        content = cfg.neoToolsToken;
        owner = "homeserver";
        group = "homeserver";
        mode = "0400";
        restartUnits = ["neo-web.service"];
      };
      systemd.services.neo-web.environment =
        lib.optionalAttrs neoToolsOn {
          NEO_AGENT_TOKEN_FILE = config.neo.core.secrets.templates."neo-agent-token".path;
        }
        // lib.optionalAttrs chatOn {
          NEO_HERMES_URL = "http://127.0.0.1:${toString cfg.gatewayPort}";
          NEO_HERMES_TOKEN_FILE = config.neo.core.secrets.templates."neo-hermes-key".path;
        };

      # The gateway key for neo-web's chat drawer (sent as Bearer to Hermes).
      neo.core.secrets.templates."neo-hermes-key" = lib.mkIf chatOn {
        content = cfg.gatewayToken;
        owner = "homeserver";
        group = "homeserver";
        mode = "0400";
        restartUnits = ["neo-web.service"];
      };

      neo.core.secrets.templates."hermes.env" = {
        env = hermesSecretEnv;
        restartUnits = ["hermes-dashboard.service"];
      };

      # Machine git identity (neo.core.git): SSH key, tokens, user.name/email.
      neo.core.git.users = ["hermes"];

      users.users.hermes = {
        extraGroups = ["wheel" "docker"];
        # Hermes runs as system units only; linger would spawn user@hermes, whose
        # dead bus makes switch-to-configuration exit 4. false (not null) also
        # disables linger left behind on existing hosts.
        linger = false;
      };

      # Agent needs unrestricted host control (docker, systemctl, package tools, …).
      # homeserver → hermes: Neo web OAuth widget (neo-hermes-auth).
      security.sudo.extraRules =
        (lib.neo.mkSudoExtraRules {
          users = ["hermes"];
          all = true;
        })
        ++ (lib.neo.mkSudoExtraRules {
          users = ["homeserver"];
          runAs = "hermes";
          commands = [
            {
              package = neoHermesAuth;
              name = "neo-hermes-auth";
            }
          ];
        });

      # Agent-oriented CLIs system-wide when Hermes is on (jq, fd, yq, …).
      environment.systemPackages = agentCliTools ++ [neoHermesAuth];

      services.hermes-agent = {
        enable = true;
        stateDir = cfg.stateDir;
        workingDirectory = "${cfg.stateDir}/workspace";
        addToSystemPackages = true;
        # Also on hermes user profile + gateway systemd PATH (see hermes-agent module).
        extraPackages = agentCliTools;
        settings = hermesSettings;
        extraDependencyGroups = ["all" "messaging" "youtube" "voice"];
        environment = hermesEnv;
        # Appended to $HERMES_HOME/.env by the hermes-agent-setup activation.
        environmentFiles = [hermesEnvFile];
        # Workspace AGENTS.md + skill tree: hermes/skills.nix.
        restart = "always";
        restartSec = 5;
      };

      # The hermes-agent module also declares the state dirs as tmpfiles rules.
      # tmpfiles refuses them under appdata (owned by homeserver, the dirs by
      # hermes: "unsafe path transition") and its hermes-agent-setup activation
      # creates the same dirs with the same modes, so drop them. An etc entry
      # replaces the generated 00-nixos.conf (see nixpkgs systemd/tmpfiles.nix).
      environment.etc."tmpfiles.d/00-nixos.conf".text = let
        underState = rule: let
          m = builtins.match "[^ ]+ +([^ ]+).*" rule;
        in
          m != null && (lib.head m == cfg.stateDir || lib.hasPrefix "${cfg.stateDir}/" (lib.head m));
      in ''
        # This file is created automatically and should not be modified.
        # Please change the option ‘systemd.tmpfiles.rules’ instead.

        ${lib.concatStringsSep "\n" (lib.filter (r: !underState r) config.systemd.tmpfiles.rules)}
      '';

      # .env is assembled at activation: read hermes.env after it is rendered.
      system.activationScripts.hermes-agent-setup.deps = ["neo-secrets" "hermes-reown"];

      # hermes has no fixed uid: a reinstall (neo restore) may allocate other
      # ids than the ones the restored appdata belongs to. Re-own what still
      # carries the old ids (the state dir's owner) before anything writes there.
      system.activationScripts.hermes-reown = lib.stringAfter ["users" "groups"] ''
        d=${lib.escapeShellArg cfg.stateDir}
        if [ -d "$d" ] && u=$(${pkgs.coreutils}/bin/id -u hermes 2>/dev/null); then
          g=$(${pkgs.coreutils}/bin/id -g hermes)
          ou=$(${pkgs.coreutils}/bin/stat -c %u "$d")
          og=$(${pkgs.coreutils}/bin/stat -c %g "$d")
          if [ "$ou" != 0 ] && [ "$ou" != "$u" ]; then
            echo "hermes: re-owning $d from uid $ou to $u"
            ${pkgs.findutils}/bin/find "$d" -xdev -user "$ou" -exec ${pkgs.coreutils}/bin/chown -h "$u" {} +
          fi
          if [ "$og" != 0 ] && [ "$og" != "$g" ]; then
            echo "hermes: re-owning $d from gid $og to $g"
            ${pkgs.findutils}/bin/find "$d" -xdev -group "$og" -exec ${pkgs.coreutils}/bin/chgrp -h "$g" {} +
          fi
        fi
      '';

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
      # It holds the dashboard password: rendered at activation, readable by SWAG only.
      neo.core.secrets.templates."hermes-autologin.html" = {
        content = autologinHtml;
        owner = toString config.neo.core.uid;
        group = toString config.neo.core.gid;
        mode = "0440";
        copies = ["${config.neo.services.swag.appdata}/www/hermes-autologin.html"];
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
          EnvironmentFile = hermesEnvFile;
        };

        environment = hermesEnv;
        # Match the gateway unit: dashboard PTYs/skills inherit this PATH too.
        path = agentCliTools;
      };
    };
  };
}
