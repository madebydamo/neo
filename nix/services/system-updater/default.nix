# System auto-updater: scheduled neo update/activate.
# The config repo is bootstrapped by neo-bootstrap (neo web service).
{self, ...}: {
  flake.modules.nixos.system-updater = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.services.system-updater;
    updaterPaths = lib.neo.mkUpdaterPaths config.neo.core.volumes.appdata;
    systemHistoryDir = cfg.appdata;
    neo = self.packages.${pkgs.stdenv.hostPlatform.system}.neo;
    activationMode = cfg.activationMode;
    rebootAfterBoot = cfg.rebootAfterBoot;
    rebootWhenRequired = cfg.rebootWhenRequired;
    # Check after a restart an update needed (restart-check.sh).
    restartCheck = pkgs.writeShellScriptBin "neo-restart-check" ''
      PATH=${lib.makeBinPath [pkgs.coreutils pkgs.gawk pkgs.jq pkgs.systemd]}:$PATH
      export NEO_UPDATER_DIR=''${NEO_UPDATER_DIR:-${systemHistoryDir}}
      exec ${pkgs.bash}/bin/bash ${./restart-check.sh} "$@"
    '';
    path = [
      neo
      pkgs.git
      pkgs.nix
      pkgs.nixos-rebuild
      pkgs.nixos-install-tools
      pkgs.coreutils
      pkgs.bash
      pkgs.jq
      pkgs.systemd
    ];
    environment = {
      NIX_BINARY_PATH = "${pkgs.nix}/bin/nix";
      SUDO_BINARY_PATH = "/run/wrappers/bin/sudo";
      # Scheduled run: wait (up to 1h) for a running activation / restore / … to
      # finish instead of failing on the operation lock like an interactive call.
      NEO_LOCK_WAIT = "3600";
      NEO_ACTIVATION_MODE = activationMode;
    };
  in {
    config = lib.mkIf cfg.enabled {
      # Keep build outputs of live derivations so build-time deps (e.g. crane/cargo
      # crate builds) are not recompiled after each scheduled GC.
      nix.settings.keep-outputs = lib.mkIf (cfg.garbageCollectOlderThan != null) true;

      system.activationScripts.neo-system-updater-history = lib.neo.mkUpdaterHistoryDirs config [updaterPaths.stateDir systemHistoryDir];

      systemd.services.neo-auto-update = {
        description = "Auto update and activate nixos config with neo";
        # neo-bootstrap is part of the neo web service. Wait for it when that
        # service is on so a scheduled update does not start without a config repo.
        wants = lib.optional config.neo.services.neo.enabled "neo-bootstrap.service";
        after = lib.optional config.neo.services.neo.enabled "neo-bootstrap.service";
        inherit path;
        inherit environment;
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = false;
          User = "homeserver";
          Group = "homeserver";
        };
        stopIfChanged = false;
        restartIfChanged = false;
        script =
          ''
            ${lib.neo.updaterRunPrelude systemHistoryDir}
            before=$(readlink /nix/var/nix/profiles/system || echo none)
            after=none
            running=none
            rc=""
            changed=false
            failed=false
            reboot_pending=false
            restart_reasons='[]'
            restart_not_applied=false
            reboot_scheduled=false
            reboot_scheduled_at=""
            finished_at=""
            activation_mode=${lib.escapeShellArg activationMode}
            reboot_when_required=${lib.boolToString rebootWhenRequired}
            reboot_after_boot=${lib.boolToString rebootAfterBoot}
            # One automatic restart per system and reasons (restart_attempted).
            attempt_file=${systemHistoryDir}/restart-attempt.json
            hist_done=0

            write_hist() {
              local in_progress=$1
              ${pkgs.jq}/bin/jq -n \
                --arg run_id "$run_id" \
                --arg started "$started_at" \
                --arg before "$before" \
                --arg after "$after" \
                --arg running "$running" \
                --arg activation_mode "$activation_mode" \
                --arg reboot_scheduled_at "$reboot_scheduled_at" \
                --argjson in_progress "$in_progress" \
                --argjson changed "$changed" \
                --argjson failed "$failed" \
                --argjson reboot_pending "$reboot_pending" \
                --argjson restart_reasons "$restart_reasons" \
                --argjson restart_not_applied "$restart_not_applied" \
                --argjson reboot_scheduled "$reboot_scheduled" \
                --arg rc "$rc" \
                --arg finished "$finished_at" \
                --arg log "$run_id.log" \
                '{kind:"system",run_id:$run_id,started_at:$started,in_progress:$in_progress,generation_before:$before,generation_after:$after,running_system:$running,activation_mode:$activation_mode,changed:$changed,failed:$failed,reboot_pending:$reboot_pending,restart_reasons:$restart_reasons,restart_not_applied:$restart_not_applied,reboot_scheduled:$reboot_scheduled,reboot_scheduled_at:(if $reboot_scheduled_at == "" then null else $reboot_scheduled_at end),exit_code:(if $rc == "" then null else ($rc|tonumber) end),finished_at:(if $finished == "" then null else $finished end),log:$log}' \
                > "$hist"
            }

            # reboot_pending: the system profile needs a restart to take full effect
            # (nix/modules/core/restart.nix: boot default ahead, kernel, Docker on old
            # settings, raised restart triggers). Store paths on both sides: the
            # profile link alone reads system-N-link and never equals a store path.
            check_restart() {
              local script=/nix/var/nix/profiles/system/sw/bin/neo-restart-required status
              if [ -x "$script" ] && status=$("$script" --json); then
                reboot_pending=true
              elif [ -x "$script" ]; then
                reboot_pending=false
              else
                # A system built before the restart check: only the boot default counts.
                status='{"reasons":[]}'
                if [ "$(readlink -f /nix/var/nix/profiles/system)" != "$(readlink -f /run/current-system)" ]; then
                  reboot_pending=true
                  status='{"reasons":[{"id":"system","text":"The new version is the boot default and runs after a restart."}]}'
                else
                  reboot_pending=false
                fi
              fi
              restart_reasons=$(${pkgs.jq}/bin/jq -c '.reasons // []' <<<"$status" 2>/dev/null || echo '[]')
            }

            finalize() {
              [ "$hist_done" -eq 1 ] && return
              hist_done=1
              trap - EXIT INT TERM HUP
              after=$(readlink /nix/var/nix/profiles/system || echo none)
              running=$(readlink -f /run/current-system || echo none)
              if [ "$before" != "$after" ]; then changed=true; fi
              check_restart
              if [ "''${1:-}" = abort ]; then
                failed=true
                if [ -z "$rc" ]; then rc=1; fi
              elif [ -n "$rc" ] && [ "$rc" -ne 0 ]; then
                failed=true
              fi
              finished_at=$(date -Iseconds)
              write_hist false
              echo "Wrote $hist (changed=$changed failed=$failed reboot_pending=$reboot_pending reasons=$restart_reasons reboot_scheduled=$reboot_scheduled mode=$activation_mode exit=$rc); last.json -> $run_id.json"
            }

            # After a successful update whose system needs a restart, schedule a delayed
            # host cycle. The delay lets this oneshot exit (history + OnSuccess
            # supervise) first. Never after a failed run, never twice for the same
            # system and reasons: if a restart did not clear them, it would not the
            # second time either (restart_not_applied, Hermes reports it).
            maybe_schedule_reboot() {
              if [ "$failed" = true ]; then
                return 0
              fi
              if [ "$reboot_pending" != true ]; then
                echo "No restart needed"
                return 0
              fi
              echo "Restart needed: $(${pkgs.jq}/bin/jq -r 'map(.text) | join(" ")' <<<"$restart_reasons")"
              if [ "$activation_mode" = boot ] && [ "$reboot_after_boot" != true ]; then
                echo "rebootAfterBoot=false; leaving it for a manual restart"
                return 0
              fi
              if [ "$activation_mode" != boot ] && [ "$reboot_when_required" != true ]; then
                echo "rebootWhenRequired=false; leaving it for a manual restart"
                return 0
              fi
              local key boot_id
              key="$(readlink -f /nix/var/nix/profiles/system) $(${pkgs.jq}/bin/jq -r 'map(.id) | sort | join(",")' <<<"$restart_reasons")"
              boot_id=$(cat /proc/sys/kernel/random/boot_id)
              if [ -f "$attempt_file" ] \
                && [ "$(${pkgs.jq}/bin/jq -r .key "$attempt_file" 2>/dev/null)" = "$key" ] \
                && [ "$(${pkgs.jq}/bin/jq -r .boot_id "$attempt_file" 2>/dev/null)" != "$boot_id" ]; then
                echo "WARNING: already restarted once for this version and the same reasons, and they are still there; not restarting again" >&2
                restart_not_applied=true
                write_hist false
                return 0
              fi
              ${pkgs.jq}/bin/jq -n --arg key "$key" --arg boot_id "$boot_id" --arg at "$(date -Iseconds)" \
                '{key: $key, boot_id: $boot_id, at: $at}' > "$attempt_file"
              schedule_reboot "mode=$activation_mode"
            }

            schedule_reboot() {
              # /run/neo/restart-dry-run (tests): decide, but do not restart.
              if [ -e /run/neo/restart-dry-run ]; then
                echo "Dry run: would schedule a host cycle in 60s ($1)"
                return 0
              fi
              echo "Scheduling delayed host cycle ($1, unit=neo-auto-update-apply, on-active=60s)"
              # Clear a leftover transient unit from a prior run (best-effort).
              /run/wrappers/bin/sudo systemctl reset-failed neo-auto-update-apply.service 2>/dev/null || true
              /run/wrappers/bin/sudo systemctl stop neo-auto-update-apply.timer 2>/dev/null || true
              /run/wrappers/bin/sudo systemctl stop neo-auto-update-apply.service 2>/dev/null || true
              if /run/wrappers/bin/sudo systemd-run \
                --on-active=60s \
                --unit=neo-auto-update-apply \
                --description="neo-auto-update delayed restart for the update" \
                /run/current-system/sw/bin/systemctl reboot; then
                reboot_scheduled=true
                reboot_scheduled_at=$(date -Iseconds)
                write_hist false
                echo "Delayed host cycle scheduled at $reboot_scheduled_at (reboot_scheduled=true); host should reboot in ~60s"
              else
                echo "ERROR: failed to schedule delayed host cycle via systemd-run" >&2
              fi
            }

            write_hist true
            ln -sfn "$run_id.json" ${systemHistoryDir}/last.json
            echo "Started run $run_id; last.json -> $run_id.json (in_progress) activation_mode=$activation_mode reboot_after_boot=$reboot_after_boot"
            trap 'finalize abort' EXIT INT TERM HUP
          ''
          + lib.optionalString (cfg.garbageCollectOlderThan != null) ''
            /run/wrappers/bin/sudo nix-collect-garbage --delete-older-than ${cfg.garbageCollectOlderThan} || true
          ''
          + ''
            set +e
            ${neo}/bin/neo --profile server update && ${neo}/bin/neo --profile server activate --mode "$activation_mode"
            rc=$?
            set -e
            finalize ok
            maybe_schedule_reboot
            # Check the server once it runs again after the restart this run
            # needs, whenever that happens (neo-auto-update-verify). With a
            # scheduled restart, Hermes supervises the run then instead of now.
            if [ "$failed" != true ] && [ "$reboot_pending" = true ]; then
              ${restartCheck}/bin/neo-restart-check record "$run_id" "$restart_reasons" "$activation_mode" "$reboot_scheduled" \
                || echo "WARNING: could not record the check after the restart" >&2
            elif [ "$failed" != true ]; then
              ${restartCheck}/bin/neo-restart-check forget || true
            fi
            exit "$rc"
          '';
      };

      # A few minutes after every boot: when the last update needed this
      # restart, record whether the server came back on the new system
      # (restart_check in the run's history; Hermes reads it).
      systemd.services.neo-auto-update-verify = {
        description = "Check the server after a restart an update needed";
        restartIfChanged = false;
        serviceConfig = {
          Type = "oneshot";
          User = "homeserver";
          Group = "homeserver";
          TimeoutStartSec = "20min";
          ExecStart = "${restartCheck}/bin/neo-restart-check verify";
        };
      };

      systemd.timers.neo-auto-update-verify = {
        wantedBy = ["timers.target"];
        timerConfig = {
          OnBootSec = "2min";
          AccuracySec = "30s";
          Unit = "neo-auto-update-verify.service";
        };
      };

      environment.systemPackages = [restartCheck];

      systemd.timers.neo-auto-update = {
        wantedBy = ["timers.target"];
        timerConfig = {
          OnCalendar = cfg.schedule;
          Persistent = true;
          RandomizedDelaySec = "5m";
          AccuracySec = "1m";
          Unit = "neo-auto-update.service";
        };
      };

      # homeserver runs neo update/activate and optional GC as this user.
      # (web UI store repair / journalctl / systemd-run / rm live on neo.services.neo)
      security.sudo.extraRules = lib.neo.mkSudoExtraRules {
        users = ["homeserver"];
        commands = [
          # neo activate → sudo nixos-rebuild switch|boot
          {
            package = pkgs.nixos-rebuild;
            name = "nixos-rebuild";
          }
          # neo activate → systemctl reset-failed/stop of rebuild unit;
          # delayed boot-apply → systemctl reboot (via systemd-run unit)
          {
            package = pkgs.systemd;
            name = "systemctl";
          }
          # schedule delayed host cycle after boot activate
          {
            package = pkgs.systemd;
            name = "systemd-run";
          }
          # scheduled GC before update (`sudo nix-collect-garbage --delete-older-than …`)
          {
            package = pkgs.nix;
            name = "nix-collect-garbage";
          }
        ];
      };
    };
  };
}
