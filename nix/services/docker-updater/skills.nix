# Hermes skill for docker-updater.
{...}: {
  flake.modules.nixos.docker-updater-skills = {
    config,
    lib,
    ...
  }: let
    updaterDir = "${config.neo.core.volumes.appdata}/updater";
  in {
    config.neo.services.docker-updater.skill.conf = lib.neo.mkServiceSkill {
      service = "docker-updater";
      inherit config;
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
        5. Roll back a broken update: `sudo neo-docker-rollback --previous --image '<repo:tag>'` (retag + restart; settings unchanged, the next run moves forward again)
        6. Return apps to what ran at a time (e.g. a restored snapshot): `sudo neo-docker-rollback --at <RFC 3339 or epoch> [--image REF | --unit docker-x]`; versions no longer on disk are pulled by digest
        7. Which image ran when: `/var/lib/neo/docker-images.json` (image ledger; a copy per run in `${updaterDir}/docker/images.json`)

        ## Pitfalls
        - `latest` tags can surprise; pin tags in service containers options for stability (the user decides; never pin for them)
        - Each run first deletes replaced versions older than `garbageCollectOlderThan` (default 30d) and every image no container uses, including hand-built or pulled images not yet started. Volumes are never touched
        - Supervision skips Hermes when no image ID changed

        ## Verification
        - Images refreshed; services still healthy after restart
        - A new file appeared under `${updaterDir}/docker/`
      '';
    };
  };
}
