# Hermes skill for backup.
{...}: {
  flake.modules.nixos.backup-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.backup;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.backup.skill.conf = lib.neo.mkServiceSkill {
      service = "backup";
      inherit cfg domain;
      description = "rsync-over-SSH backup of Neo data";
      tags = ["neo" "backup"];
      body = ''
        ## When to Use
        Off-site/remote backups, schedule, excludes, failed backup runs.

        ## Architecture notes
        - Source default: neo volumes root
        - Transport: rsync over SSH to remote host
        - Options: host, user, sshKey, remotePath, schedule, excludedDirs

        ## Credentials
        - Settings: `services.backup` SSH host/user/key (see ssh connection options)
        - Default key is often the homeserver auto-generated key — confirm path in settings

        ## Procedures
        1. Verify SSH to remote works with the configured key
        2. Run one-shot backup unit
        3. Confirm remote path contents

        ## Pitfalls
        - Root runs backup for source readability; key permissions matter
        - Excludes must be correct to avoid huge or incomplete backups

        ## Verification
        - Timer scheduled; last log shows success; remote listing OK
      '';
    };
  };
}
