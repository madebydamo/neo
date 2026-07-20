# Hermes skill for system-updater.
{...}: {
  flake.modules.nixos.system-updater-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.system-updater;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.system-updater.skill.conf = lib.neo.mkServiceSkill {
      service = "system-updater";
      inherit cfg domain;
      description = "Scheduled neo update/activate and GC";
      tags = ["neo" "updates"];
      body = ''
        ## When to Use
        OS/Neo module updates, bootstrap config repo, garbage collection schedule.

        ## CLI extras
        ```bash
        systemctl list-timers | grep neo
        # manual
        neo update && neo activate
        ```

        ## Credentials
        - Uses host neo CLI / config path; no extra API keys

        ## Procedures
        1. Check timer schedule (`services.system-updater.schedule`)
        2. Manual update when needed
        3. Inspect GC setting `garbageCollectOlderThen` (default 30d; null disables GC)
        4. With GC on, `nix.settings.keep-outputs` is enabled so live build-time outputs stay until their generation ages out

        ## Pitfalls
        - Updates can break services; prefer dry-run when testing major changes
        - Ensure config repo is healthy before scheduled activate
        - GC without keep-outputs deletes unrooted build-time deps (slow Rust rebuilds)

        ## Verification
        - Timer active; last run logs clean; system generation recent
      '';
    };
  };
}
