# Hermes skill for filebrowser.
{...}: {
  flake.modules.nixos.filebrowser-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.filebrowser.skill.conf = lib.neo.mkServiceSkill {
      service = "filebrowser";
      inherit config;
      description = "Filebrowser web file manager mounts";
      tags = ["neo" "filebrowser" "files"];
      body = ''
        ## When to Use
        Browse/upload files on configured host mounts (Documents etc.).

        ## Architecture notes
        - Default mounts: media → /srv/Media, documents → /srv/Documents, appdata → /srv/AppData
        - Extra mounts: `additionalMountPoints` list of `{ localPath, containerPath }`

        ## Credentials
        - App users may be app-managed; edge tinyauth typically required
        - No Neo password option by default

        ## Procedures
        1. Health-check
        2. Confirm default mounts under /srv; add extras via additionalMountPoints if needed
        3. Fix permissions on host volumes if uploads fail

        ## Pitfalls
        - Deleting via UI deletes real host files

        ## Verification
        - UI lists expected directories; upload works
      '';
    };
  };
}
