# Hermes update supervision: oneshots after system/docker updater runs.
# Gated by neo.services.hermes.superviseUpdates (default false).
{...}: {
  flake.modules.nixos.hermes-supervise = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.services.hermes;
    supervise = cfg.enabled && cfg.superviseUpdates;
    hermesPkg = config.services.hermes-agent.package;
    updaterPaths = lib.neo.mkUpdaterPaths config.neo.core.volumes.appdata;
    updaterStateDir = updaterPaths.stateDir;
    dockerManifest = "${config.neo.services.docker-updater.appdata}/last.json";
    systemManifest = "${config.neo.services.system-updater.appdata}/last.json";

    superviseEnv = lib.filterAttrs (_: v: v != null && v != "") {
      HOME = cfg.stateDir;
      HERMES_HOME = "${cfg.stateDir}/.hermes";
      XAI_API_KEY = cfg.xaiApiKey;
      ANTHROPIC_API_KEY = cfg.anthropicApiKey;
      OPENAI_API_KEY = cfg.openaiApiKey;
      OPENROUTER_API_KEY = cfg.openrouterApiKey;
      TELEGRAM_BOT_TOKEN = cfg.telegramBotToken;
      HERMES_GATEWAY_TOKEN = cfg.gatewayToken;
      TELEGRAM_ALLOWED_USERS = lib.concatStringsSep "," (map toString cfg.telegramAllowedUserId);
    };

    promptFor = kind: ''
      You are supervising a Neo homeserver ${kind} update. Load skill /neo-update-supervisor and follow it exactly.

      Latest marker (symlink, retargeted at run start): ${
        if kind == "system"
        then systemManifest
        else dockerManifest
      }
      Run history (JSON + matching .log per run): ${
        if kind == "system"
        then config.neo.services.system-updater.appdata
        else config.neo.services.docker-updater.appdata
      }
      If the marker has in_progress=true, the updater did not finish cleanly — treat as failed.
      Updater unit: ${
        if kind == "system"
        then "neo-auto-update.service"
        else "neo-docker-updater.service"
      }

      Classify the outcome as broken, warning, or clean using systemd state and logs.
      - clean: do not send any message
      - warning (deprecations, migration hints, non-fatal issues): notify via `hermes send --to all`; do not roll anything back
      - broken: notify via `hermes send --to all`
        ${
        if kind == "docker"
        then "- for each broken image, run `sudo neo-docker-rollback --image '<repo:tag>'` then confirm the unit is active; mention the rollback in the notification"
        else "- do NOT roll back the NixOS generation; only report what failed so the operator can fix it"
      }

      Never dump secrets. Keep the notification short and operational.
    '';

    systemPromptFile = pkgs.writeText "neo-hermes-supervise-system-prompt.txt" (promptFor "system");
    dockerPromptFile = pkgs.writeText "neo-hermes-supervise-docker-prompt.txt" (promptFor "docker");

    superviseScript = pkgs.writeShellScriptBin "neo-hermes-supervise" ''
      set -euo pipefail
      kind=''${1:-}
      case "$kind" in
        system)
          marker=${systemManifest}
          unit=neo-auto-update.service
          prompt_file=${systemPromptFile}
          ;;
        docker)
          marker=${dockerManifest}
          unit=neo-docker-updater.service
          prompt_file=${dockerPromptFile}
          ;;
        *)
          echo "usage: neo-hermes-supervise system|docker" >&2
          exit 2
          ;;
      esac

      changed=false
      failed=false
      in_progress=false
      if [ -f "$marker" ]; then
        changed=$(${pkgs.jq}/bin/jq -r '.changed // false' "$marker")
        failed=$(${pkgs.jq}/bin/jq -r '.failed // false' "$marker")
        in_progress=$(${pkgs.jq}/bin/jq -r '.in_progress // false' "$marker")
        echo "marker $marker changed=$changed failed=$failed in_progress=$in_progress"
        ${pkgs.jq}/bin/jq . "$marker" || true
      else
        echo "no marker at $marker"
      fi

      unit_failed=false
      if ${pkgs.systemd}/bin/systemctl is-failed --quiet "$unit" 2>/dev/null; then
        unit_failed=true
        echo "$unit is failed"
      fi

      if [ "$changed" != true ] && [ "$failed" != true ] && [ "$in_progress" != true ] && [ "$unit_failed" != true ]; then
        echo "noop: skip Hermes"
        exit 0
      fi

      echo "Launching Hermes supervisor for $kind"
      ${hermesPkg}/bin/hermes --yolo chat -Q --source tool --max-turns 40 \
        -s neo-update-supervisor --query-file "$prompt_file"
    '';
  in {
    config = lib.mkIf supervise {
      environment.systemPackages = [superviseScript];

      system.activationScripts.neo-updater-state = lib.neo.mkEnsureDirs config [
        {
          dirPath = updaterStateDir;
          mode = "0775";
          user = "homeserver";
          group = "homeserver";
        }
        {
          dirPath = updaterPaths.dockerHistoryDir;
          mode = "0775";
          user = "homeserver";
          group = "homeserver";
        }
        {
          dirPath = updaterPaths.systemHistoryDir;
          mode = "0775";
          user = "homeserver";
          group = "homeserver";
        }
      ];

      systemd.services.neo-hermes-supervise-system-update = {
        description = "Hermes supervision of neo-auto-update";
        after = ["network-online.target" "neo-auto-update.service"];
        wants = ["network-online.target"];
        path = [pkgs.jq pkgs.systemd pkgs.sudo hermesPkg];
        environment = superviseEnv;
        serviceConfig = {
          Type = "oneshot";
          User = "hermes";
          Group = "hermes";
          WorkingDirectory = "${cfg.stateDir}/workspace";
          TimeoutStartSec = "15min";
          ExecStart = "${superviseScript}/bin/neo-hermes-supervise system";
        };
      };

      systemd.services.neo-hermes-supervise-docker-update = {
        description = "Hermes supervision of neo-docker-updater";
        after = ["network-online.target" "neo-docker-updater.service"];
        wants = ["network-online.target"];
        path = [pkgs.jq pkgs.systemd pkgs.sudo pkgs.docker hermesPkg];
        environment = superviseEnv;
        serviceConfig = {
          Type = "oneshot";
          User = "hermes";
          Group = "hermes";
          WorkingDirectory = "${cfg.stateDir}/workspace";
          TimeoutStartSec = "15min";
          ExecStart = "${superviseScript}/bin/neo-hermes-supervise docker";
        };
      };

      systemd.services.neo-auto-update = lib.mkIf (config.neo.services.system-updater.enabled or false) {
        onSuccess = ["neo-hermes-supervise-system-update.service"];
        onFailure = ["neo-hermes-supervise-system-update.service"];
      };

      systemd.services.neo-docker-updater = lib.mkIf (config.neo.services.docker-updater.enabled or false) {
        onSuccess = ["neo-hermes-supervise-docker-update.service"];
        onFailure = ["neo-hermes-supervise-docker-update.service"];
      };
    };
  };
}
