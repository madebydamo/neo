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
      description = "Backup of Neo data over SSH or to a disk: plain files, encrypted (restic), or ZFS snapshots";
      tags = ["neo" "backup"];
      body = ''
        ## When to Use
        Off-site/remote backups, schedule, excludes, failed backup runs.

        ## Architecture notes
        - Source default: neo volumes root, copied from a ZFS snapshot on the Disko layout
        - format files: rsync to remotePath/neo; zfs: zfs send to dataset; encrypted: restic repository remotePath/restic-<UTC> plus remotePath/neo-backup.age
        - neo-backup.age holds the restic password, age-encrypted to the SSH host key and the ssh-ed25519 core.ssh.authorizedKeys; re-encrypted every run
        - Options: target, format, host, user, sshKey, remotePath, dataset, keep, schedule, excludedDirs

        ## Credentials
        - Settings: `services.backup` SSH host/user/key (see ssh connection options)
        - Default login key is the SSH host key; its public key must be in the backup computer's authorized_keys
        - Encrypted format has no password setting: the key is in neo-backup.age

        ## Procedures
        1. Verify SSH to remote works with the configured key
        2. Run one-shot backup unit (`systemctl start backup`), read `journalctl -u backup`
        3. Confirm remote path contents
        4. New encryption key (after removing an admin key): `neo backup rotate-key` (full backup, then the old repository is deleted)
        5. Config repo lost: `neo backup extract-config user@host:folder --key <admin key>` on a computer with the admin key

        ## Pitfalls
        - Root runs backup for source readability; key permissions matter
        - Excludes must be correct to avoid huge or incomplete backups
        - Encrypted: a neo-backup.age the host key cannot open is never overwritten; pick another folder
        - Encrypted over SSH needs SFTP at the destination; removing an old repository needs a shell (else delete by hand)
        - Remote dirs are forced owner-writable (`--chmod=Du+w`); a failed delete on a read-only remote dir is chmod-ed over SSH and retried once

        ## Verification
        - Timer scheduled; last log shows success; remote listing OK
      '';
    };
  };
}
