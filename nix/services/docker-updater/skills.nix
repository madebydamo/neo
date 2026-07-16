# Hermes skill for docker-updater.
{...}: {
  flake.modules.nixos.docker-updater-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.docker-updater;
    domain = config.neo.services.swag.domain or null;
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
        2. Trigger one-shot update unit
        3. Verify containers restarted and healthy

        ## Pitfalls
        - `latest` tags can surprise; pin tags in service containers options for stability

        ## Verification
        - Images refreshed; services still healthy after restart
      '';
    };
  };
}
