# Hermes skill for system-updater.
{...}: {
  flake.modules.nixos.system-updater-skills = {
    config,
    lib,
    ...
  }: let
    updaterDir = "${config.neo.core.volumes.appdata}/updater";
  in {
    config.neo.services.system-updater.skill.conf = lib.neo.mkServiceSkill {
      service = "system-updater";
      inherit config;
      description = "Scheduled neo update/activate and GC";
      tags = ["neo" "updates"];
      body = ''
        ## When to Use
        OS/Neo module updates and the garbage collection schedule. The config repo unit is neo-bootstrap on the neo web service.

        ## CLI extras
        ```bash
        systemctl list-timers | grep neo
        # manual (interactive default is live switch)
        neo update && neo activate
        # boot-only activate (no live switch; reboot later unless rebootAfterBoot)
        neo activate --mode boot
        # or NEO_ACTIVATION_MODE=boot neo activate
        ```

        ## Credentials
        - Uses host neo CLI / config path; no extra API keys

        ## Procedures
        1. Check timer schedule (`services.system-updater.schedule`)
        2. Manual update when needed
        3. Inspect GC setting `garbageCollectOlderThan` (default 30d; null disables GC)
        4. With GC on, `nix.settings.keep-outputs` is enabled so live build-time outputs stay until their generation ages out
        5. `activationMode`: `switch` (default, live nixos-rebuild switch) or `boot` (nixos-rebuild boot only — profile advances, running system until reboot). Prefer `boot` on edge boxes where unattended live switch freezes dhcpcd/journald. Interactive web Activate stays switch unless you pass the flag.
        6. Restart decision: `neo-restart-required [--json]` says whether the system profile needs a restart and why (`system` = boot default ahead, `kernel`, `docker` = dockerd still on an older daemon.json, `trigger:<id>` = a component raised its restart trigger). After a successful scheduled run that needs one, `neo-auto-update` schedules a delayed host restart (~60s via transient `neo-auto-update-apply`): with `activationMode=boot` when `rebootAfterBoot` (default true), with `switch` when `rebootWhenRequired` (default true). Never for no-op updates or a failed activate, never twice for the same system and reasons (`restart-attempt.json`; then `restart_not_applied=true`). `touch /run/neo/restart-dry-run` makes it log the decision only. Interactive web Activate never restarts; it shows the reasons.
        7. Run history is this service's appdata `${updaterDir}/system/` (`<utc>-<pid>.json` + `.log`). JSON records `activation_mode`, `reboot_pending` + `restart_reasons` (from `neo-restart-required`), `restart_not_applied`, and when applicable `reboot_scheduled` + `reboot_scheduled_at`. `${updaterDir}/system/last.json` is retargeted at run start (in-progress stub) so a crash never leaves it on an older run. Clear appdata in Neo web wipes the history.
        8. If Hermes `superviseUpdates` is on: Hermes notifies on failure/warnings and never auto-rollbacks the generation

        ## Pitfalls
        - Updates can break services; prefer dry-run when testing major changes
        - Ensure config repo is healthy before scheduled activate
        - GC without keep-outputs deletes unrooted build-time deps (slow Rust rebuilds)
        - With `activationMode=boot` and `rebootAfterBoot=false`, a successful run leaves `reboot_pending=true` until a manual reboot; do not treat profile change alone as "already live"
        - With `rebootAfterBoot=true`, expect a host cycle ~1 min after a successful boot-mode run that changed the generation; OnSuccess supervise runs in the delay window
        - In switch mode a restart happens only when `restart_reasons` is non-empty; a new generation alone never restarts the host

        ## Verification
        - Timer active; last run logs clean; system generation recent
        - mode=switch: `/run/current-system` matches profile after run; a host cycle only with non-empty `restart_reasons`
        - mode=boot + rebootAfterBoot=true + real change: hist shows `reboot_pending` and `reboot_scheduled`; host cycles within ~1–2 min; new gen is `/run/current-system` after boot
        - mode=boot + rebootAfterBoot=false: profile advanced, running old until manual cycle; `reboot_scheduled=false`
        - no-op update or failed activate: no host cycle
      '';
    };
  };
}
