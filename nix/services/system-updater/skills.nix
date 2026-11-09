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
        6. `rebootAfterBoot` (default true): only for scheduled `neo-auto-update` when `activationMode=boot`. After a successful activate that leaves `reboot_pending=true`, schedules a delayed host restart (~60s via transient `neo-auto-update-apply`) so the new generation becomes live. Set false to keep boot-only until a manual reboot. Never auto-cycles on `activationMode=switch`, no-op updates, or failed activate. Interactive web Activate does not auto-cycle.
        7. Run history is this service's appdata `${updaterDir}/system/` (`<utc>-<pid>.json` + `.log`). JSON records `activation_mode`, `reboot_pending` (profile vs `/run/current-system`), and when applicable `reboot_scheduled` + `reboot_scheduled_at`. `${updaterDir}/system/last.json` is retargeted at run start (in-progress stub) so a crash never leaves it on an older run. Clear appdata in Neo web wipes the history.
        8. If Hermes `superviseUpdates` is on: Hermes notifies on failure/warnings and never auto-rollbacks the generation

        ## Pitfalls
        - Updates can break services; prefer dry-run when testing major changes
        - Ensure config repo is healthy before scheduled activate
        - GC without keep-outputs deletes unrooted build-time deps (slow Rust rebuilds)
        - With `activationMode=boot` and `rebootAfterBoot=false`, a successful run leaves `reboot_pending=true` until a manual reboot; do not treat profile change alone as "already live"
        - With `rebootAfterBoot=true`, expect a host cycle ~1 min after a successful boot-mode run that changed the generation; OnSuccess supervise runs in the delay window

        ## Verification
        - Timer active; last run logs clean; system generation recent
        - mode=switch: `/run/current-system` matches profile after run; no auto host cycle from this option
        - mode=boot + rebootAfterBoot=true + real change: hist shows `reboot_pending` and `reboot_scheduled`; host cycles within ~1–2 min; new gen is `/run/current-system` after boot
        - mode=boot + rebootAfterBoot=false: profile advanced, running old until manual cycle; `reboot_scheduled=false`
        - no-op update or failed activate: no host cycle
      '';
    };
  };
}
