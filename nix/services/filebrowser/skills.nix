# Hermes skill for filebrowser.
{...}: {
  flake.modules.nixos.filebrowser-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.filebrowser;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.filebrowser.skill.conf = lib.neo.mkServiceSkill {
      service = "filebrowser";
      inherit cfg domain;
      description = "Filebrowser web file manager mounts";
      tags = ["neo" "filebrowser" "files"];
      body = ''
        ## When to Use
        Browse/upload files on configured host mounts (Documents etc.).

        ## Architecture notes
        - Mounts neo volumes (e.g. documents) into container

        ## Credentials
        - App users may be app-managed; edge tinyauth typically required
        - No Neo password option by default

        ## Procedures
        1. Health-check
        2. Confirm mounts in settings (additionalMountPoints if any)
        3. Fix permissions on host volumes if uploads fail

        ## Pitfalls
        - Deleting via UI deletes real host files

        ## Verification
        - UI lists expected directories; upload works
      '';
    };
  };
}
