# Hermes skill for docker-updater.
{...}: {
  flake.modules.nixos.docker-updater-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.docker-updater;
    domain = config.neo.services.swag.domain or null;
    updaterDir = "${config.neo.core.volumes.appdata}/updater";
  in {
    config.neo.services.docker-updater.skill.conf = lib.neo.mkServiceSkill {
      service = "docker-updater";
      inherit cfg domain;
      description = "Pull newer OCI images for declared containers";
      tags = ["neo" "docker" "updates"];
      body = ''
        ## When to Use
        Refresh container images for enabled services without full system update.

        ## Credentials
        - None (local docker)

        ## Procedures
        1. Confirm schedule option
        2. Trigger one-shot update unit (`systemctl start neo-docker-updater`)
        3. Verify containers restarted and healthy
        4. Run history is this service's appdata `${updaterDir}/docker/` (`<utc>-<pid>.json` + `.log`). `${updaterDir}/docker/last.json` is retargeted at run start (in-progress stub) so a crash never leaves it on an older run. Clear appdata in Neo web wipes the history.
        5. If Hermes `superviseUpdates` is on: rollback with `sudo neo-docker-rollback --image '<repo:tag>'` (or `--manifest` to use a historical JSON)

        ## Pitfalls
        - `latest` tags can surprise; pin tags in service containers options for stability
        - Supervision skips Hermes when no image ID changed

        ## Verification
        - Images refreshed; services still healthy after restart
        - A new file appeared under `${updaterDir}/docker/`
      '';
    };
  };
}
