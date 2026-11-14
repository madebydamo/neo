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
            reboot_scheduled=false
            reboot_scheduled_at=""
            finished_at=""
            activation_mode=${lib.escapeShellArg activationMode}
            reboot_when_required=${lib.boolToString rebootWhenRequired}
            reboot_after_boot=${
              if rebootAfterBoot
              then "true"
              else "false"
            }
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
                --argjson reboot_scheduled "$reboot_scheduled" \
                --arg rc "$rc" \
                --arg finished "$finished_at" \
                --arg log "$run_id.log" \
                '{kind:"system",run_id:$run_id,started_at:$started,in_progress:$in_progress,generation_before:$before,generation_after:$after,running_system:$running,activation_mode:$activation_mode,changed:$changed,failed:$failed,reboot_pending:$reboot_pending,reboot_scheduled:$reboot_scheduled,reboot_scheduled_at:(if $reboot_scheduled_at == "" then null else $reboot_scheduled_at end),exit_code:(if $rc == "" then null else ($rc|tonumber) end),finished_at:(if $finished == "" then null else $finished end),log:$log}' \
                > "$hist"
            }

            finalize() {
              [ "$hist_done" -eq 1 ] && return
              hist_done=1
              trap - EXIT INT TERM HUP
              after=$(readlink /nix/var/nix/profiles/system || echo none)
              running=$(readlink /run/current-system || echo none)
              if [ "$before" != "$after" ]; then changed=true; fi
              # Boot mode (or any path where profile advanced but running did not) needs a reboot.
              if [ "$after" != "none" ] && [ "$after" != "$running" ]; then
                reboot_pending=true
              else
                reboot_pending=false
              fi
              if [ "''${1:-}" = abort ]; then
                failed=true
                if [ -z "$rc" ]; then rc=1; fi
              elif [ -n "$rc" ] && [ "$rc" -ne 0 ]; then
                failed=true
              fi
              finished_at=$(date -Iseconds)
              write_hist false
              echo "Wrote $hist (changed=$changed failed=$failed reboot_pending=$reboot_pending reboot_scheduled=$reboot_scheduled mode=$activation_mode exit=$rc); last.json -> $run_id.json"
            }

            # After a successful boot-mode activate that advanced the profile, schedule a
            # delayed host cycle so the new generation becomes live. Delay lets this oneshot
            # exit (history + OnSuccess supervise) before reboot.
            maybe_schedule_reboot() {
              if [ "$failed" = true ]; then
                return 0
              fi
              if [ "$activation_mode" != boot ]; then
                # A live switch can still carry a change that needs a restart
                # (neo.core.rebootEpoch, nix/modules/core/reboot.nix).
                if [ "$reboot_when_required" = true ] && /run/current-system/sw/bin/neo-reboot-required; then
                  reboot_pending=true
                  schedule_reboot "update needs a restart"
                fi
                return 0
              fi
              if [ "$reboot_after_boot" != true ]; then
                echo "rebootAfterBoot=false; leaving reboot_pending for a manual host cycle"
                return 0
              fi
              if [ "$reboot_pending" != true ]; then
                echo "No reboot_pending (generation unchanged or already live); skipping host cycle"
                return 0
              fi
              schedule_reboot "activationMode=boot, generation changed, rebootAfterBoot=true"
            }

            schedule_reboot() {
              echo "Scheduling delayed host cycle ($1, unit=neo-auto-update-apply, on-active=60s)"
              # Clear a leftover transient unit from a prior run (best-effort).
              /run/wrappers/bin/sudo systemctl reset-failed neo-auto-update-apply.service 2>/dev/null || true
              /run/wrappers/bin/sudo systemctl stop neo-auto-update-apply.timer 2>/dev/null || true
              /run/wrappers/bin/sudo systemctl stop neo-auto-update-apply.service 2>/dev/null || true
              if /run/wrappers/bin/sudo systemd-run \
                --on-active=60s \
                --unit=neo-auto-update-apply \
                --description="neo-auto-update delayed reboot after boot activate" \
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
            exit "$rc"
          '';
      };

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
