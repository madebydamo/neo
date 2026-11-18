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

    telegramHomeChannel =
      if cfg.telegramAllowedUserId == []
      then null
      else toString (builtins.head cfg.telegramAllowedUserId);

    superviseEnv = lib.filterAttrs (_: v: v != null && v != "") {
      HOME = cfg.stateDir;
      HERMES_HOME = "${cfg.stateDir}/.hermes";
      # Same as the gateway (default.nix): users > 0; group chats < 0 and telegramGroups.
      TELEGRAM_ALLOWED_USERS = config.services.hermes-agent.environment.TELEGRAM_ALLOWED_USERS or null;
      TELEGRAM_GROUP_ALLOWED_CHATS = config.services.hermes-agent.environment.TELEGRAM_GROUP_ALLOWED_CHATS or null;
      TELEGRAM_HOME_CHANNEL = telegramHomeChannel;
    };

    # Secret half: rendered at activation, read through EnvironmentFile=.
    superviseSecretEnv =
      lib.filterAttrs (_: v: v != null && v != "") {
        TELEGRAM_BOT_TOKEN = cfg.telegramBotToken;
        HERMES_GATEWAY_TOKEN = cfg.gatewayToken;
      }
      // lib.neo.mkHermesLlmEnv {
        inherit (cfg.llm) provider apiKey;
      };
    superviseEnvFile = config.neo.core.secrets.templates."hermes-supervise.env".path;

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

      ${lib.optionalString (kind == "system") ''
        Restarts: `restart_reasons` lists why the new version needs a restart of the server
        (new Linux kernel, Docker still on old settings, a component that asked for one).
        `reboot_scheduled=true` means the server restarts by itself about a minute later:
        say so in one line, it is not a problem. Reasons without a scheduled restart: tell
        the operator to restart when convenient (Settings → Power). `restart_not_applied=true`
        means a restart already happened for the same reasons and did not clear them: that
        is a warning. An empty `restart_reasons` needs no restart, whatever `reboot_pending`
        said in older runs.
      ''}
      Classify the outcome as broken, warning, or clean using systemd state and logs.
      - clean: do not send any message
      - warning (deprecations, migration hints, non-fatal issues): notify via `hermes send --to all`; do not roll anything back
      - broken: notify via `hermes send --to all`
        ${
        if kind == "docker"
        then "- for each broken image, run `sudo neo-docker-rollback --previous --image '<repo:tag>'` then confirm the unit is active; mention the rollback in the notification"
        else "- do NOT roll back the NixOS generation; only report what failed so the operator can fix it"
      }

      Never dump secrets. Keep the notification short and operational.
    '';

    restartChecked = "${config.neo.services.system-updater.appdata}/restart-checked.json";
    restartPrompt = ''
      You are supervising a Neo homeserver after a restart that a system update needed. Load skill /neo-update-supervisor and follow it exactly.

      Run: ${restartChecked} (the history JSON of the update run that needed the restart; its log is the
      file named in `log`, next to it in ${config.neo.services.system-updater.appdata}). The run itself was not
      supervised before the restart when `reboot_scheduled=true`: classify it too (exit code, warnings in its log).
      The update ran in the previous boot (`journalctl -b -1`), this boot is `journalctl -b`.

      `restart_check` was recorded a few minutes after this boot, once it had settled:
      - `booted_expected=false`: the server did not come back on the system the update installed
        (`expected_system` vs `booted_system`), e.g. it booted an older version from the boot menu. Broken.
      - `reasons_left`: restart reasons still open (`reasons_before` were the ones before the restart). A restart
        that did not clear them is a warning; `system` or `kernel` left over is broken.
      - `failed_units`: units failed in this boot. Look at `journalctl -b -u <unit>`; broken unless clearly harmless.
      - `system_state` other than running/degraded: the boot had not finished after 15 minutes. Broken.
      - `ok=true` and a clean run log: clean, no message.
      Classify the outcome as broken, warning, or clean.
      - clean: do not send any message
      - warning or broken: notify via `hermes send --to all`, one line saying the server restarted for the update
        (with the reasons) and what is wrong
      - do NOT roll back the NixOS generation; only report what failed so the operator can fix it

      Never dump secrets. Keep the notification short and operational.
    '';
    restartPromptFile = pkgs.writeText "neo-hermes-supervise-restart-prompt.txt" restartPrompt;

    systemPromptFile = pkgs.writeText "neo-hermes-supervise-system-prompt.txt" (promptFor "system");
    dockerPromptFile = pkgs.writeText "neo-hermes-supervise-docker-prompt.txt" (promptFor "docker");

    superviseScript = pkgs.writeShellScriptBin "neo-hermes-supervise" ''
      set -euo pipefail
      kind=''${1:-}
      case "$kind" in
        restart)
          marker=${restartChecked}
          unit=neo-auto-update-verify.service
          prompt_file=${restartPromptFile}
          ;;
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
          echo "usage: neo-hermes-supervise system|docker|restart" >&2
          exit 2
          ;;
      esac

      # restart: once per update run, after the boot that restart-check verified.
      if [ "$kind" = restart ]; then
        if [ ! -f "$marker" ]; then
          echo "no restart check at $marker: skip Hermes"
          exit 0
        fi
        key=$(${pkgs.jq}/bin/jq -r '"\(.run_id) \(.restart_check.boot_id // "")"' "$marker")
        done_file="$HOME/.neo-supervise-restart-done"
        if [ "$(cat "$done_file" 2>/dev/null || true)" = "$key" ]; then
          echo "restart check $key already supervised: skip Hermes"
          exit 0
        fi
      fi

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

      # The updater restarts the server in a minute: supervise the run after
      # the restart (neo-hermes-supervise-after-restart), when it can be judged.
      if [ "$kind" = system ] && [ "$failed" != true ] && [ "$in_progress" != true ] \
        && [ "$(${pkgs.jq}/bin/jq -r '.reboot_scheduled // false' "$marker" 2>/dev/null)" = true ]; then
        echo "restart scheduled: Hermes checks this run after the restart"
        exit 0
      fi

      if [ "$kind" != restart ] && [ "$changed" != true ] && [ "$failed" != true ] && [ "$in_progress" != true ] && [ "$unit_failed" != true ]; then
        echo "noop: skip Hermes"
        exit 0
      fi

      # OnSuccess often fires while hermes-agent is still coming back from switch.
      # Wait here (after noop) so clean timer ticks stay fast.
      echo "Waiting for hermes-agent before supervise ($kind)"
      waited=0
      while ! ${pkgs.systemd}/bin/systemctl is-active --quiet hermes-agent.service 2>/dev/null; do
        if [ "$waited" -ge 120 ]; then
          echo "hermes-agent still not active after ''${waited}s; continuing" >&2
          break
        fi
        sleep 2
        waited=$((waited + 2))
      done
      # Settle so the model/provider path can come up after the unit is active.
      sleep 5

      echo "Launching Hermes supervisor for $kind"
      ${hermesPkg}/bin/hermes --yolo chat -Q --source tool --max-turns 40 \
        -s neo-update-supervisor --query-file "$prompt_file"
      if [ "$kind" = restart ]; then
        echo "$key" >"$done_file"
      fi
    '';

    # Restart covers transient gateway/model failures after switch (and ExecStart
    # overlays that only replace the supervise binary). StartLimitBurst bounds it.
    superviseServiceConfig = execStart: {
      Type = "oneshot";
      User = "hermes";
      Group = "hermes";
      WorkingDirectory = "${cfg.stateDir}/workspace";
      TimeoutStartSec = "15min";
      Restart = "on-failure";
      RestartSec = "20";
      ExecStart = execStart;
      EnvironmentFile = superviseEnvFile;
    };
    # [Unit] keys: systemd ignores StartLimitIntervalSec under [Service].
    superviseUnitConfig = {
      StartLimitBurst = 8;
      StartLimitIntervalSec = 600;
    };
  in {
    config = lib.mkIf supervise {
      environment.systemPackages = [superviseScript];

      neo.core.secrets.templates."hermes-supervise.env".env = superviseSecretEnv;

      system.activationScripts.neo-updater-state = lib.neo.mkUpdaterHistoryDirs config [
        updaterStateDir
        updaterPaths.dockerHistoryDir
        updaterPaths.systemHistoryDir
      ];

      systemd.services.neo-hermes-supervise-system-update = {
        description = "Hermes supervision of neo-auto-update";
        # hermes-agent: switch often restarts it; order + Wants so we do not race a dead gateway.
        after = ["network-online.target" "neo-auto-update.service" "hermes-agent.service"];
        wants = ["network-online.target" "hermes-agent.service"];
        path = [pkgs.jq pkgs.systemd pkgs.sudo hermesPkg];
        environment = superviseEnv;
        unitConfig = superviseUnitConfig;
        serviceConfig = superviseServiceConfig "${superviseScript}/bin/neo-hermes-supervise system";
      };

      # After the boot an update restarted into (neo-auto-update-verify).
      systemd.services.neo-hermes-supervise-after-restart = {
        description = "Hermes supervision after a restart an update needed";
        after = ["network-online.target" "neo-auto-update-verify.service" "hermes-agent.service"];
        wants = ["network-online.target" "hermes-agent.service"];
        path = [pkgs.jq pkgs.systemd pkgs.sudo hermesPkg];
        environment = superviseEnv;
        unitConfig = superviseUnitConfig;
        serviceConfig = superviseServiceConfig "${superviseScript}/bin/neo-hermes-supervise restart";
      };

      systemd.services.neo-hermes-supervise-docker-update = {
        description = "Hermes supervision of neo-docker-updater";
        after = ["network-online.target" "neo-docker-updater.service" "hermes-agent.service"];
        wants = ["network-online.target" "hermes-agent.service"];
        path = [pkgs.jq pkgs.systemd pkgs.sudo pkgs.docker hermesPkg];
        environment = superviseEnv;
        unitConfig = superviseUnitConfig;
        serviceConfig = superviseServiceConfig "${superviseScript}/bin/neo-hermes-supervise docker";
      };

      # Same unit on success and failure is intentional (classify either outcome).
      systemd.services.neo-auto-update = lib.mkIf (config.neo.services.system-updater.enabled or false) {
        onSuccess = lib.mkAfter ["neo-hermes-supervise-system-update.service"];
        onFailure = lib.mkAfter ["neo-hermes-supervise-system-update.service"];
      };

      systemd.services.neo-auto-update-verify = lib.mkIf (config.neo.services.system-updater.enabled or false) {
        onSuccess = lib.mkAfter ["neo-hermes-supervise-after-restart.service"];
        onFailure = lib.mkAfter ["neo-hermes-supervise-after-restart.service"];
      };

      systemd.services.neo-docker-updater = lib.mkIf (config.neo.services.docker-updater.enabled or false) {
        onSuccess = lib.mkAfter ["neo-hermes-supervise-docker-update.service"];
        onFailure = lib.mkAfter ["neo-hermes-supervise-docker-update.service"];
      };
    };
  };
}
