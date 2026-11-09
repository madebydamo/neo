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
        - destinations.<name>: every run copies one snapshot to each enabled destination in turn (by priority, then name); a failing one is logged and the rest still run, the unit then ends failed
        - neo restore tries the destinations in the same order and falls back to the next one
        - Per destination: format files: rsync to remotePath/neo; zfs: zfs send to dataset; encrypted: restic repository remotePath/restic-<UTC> plus remotePath/neo-backup.age
        - neo-backup.age holds the restic password, age-encrypted to the SSH host key and the ssh-ed25519 core.ssh.authorizedKeys; re-encrypted every run
        - Options: schedule, excludedDirs, sourceDir; destinations.<name>: enabled, target, format, host, user, sshKey, extraOptions, storeUrl, storeClient, remotePath, dataset, keep, priority

        ## Credentials
        - Settings: `services.backup.destinations.<name>` SSH host/user/key (see ssh connection options)
        - Default login key is the SSH host key; its public key must be in the backup computer's authorized_keys
        - Encrypted format has no password setting: the key is in neo-backup.age

        ## Procedures
        1. Verify SSH to remote works with the configured key
        2. Run one-shot backup unit (`systemctl start backup`), read `journalctl -u backup` (lines are tagged with the destination) and `/var/log/backup-<name>.log`
        3. Confirm remote path contents
        4. New encryption key (after removing an admin key): `neo backup rotate-key` (every encrypted destination: full backup, then the old repository is deleted)
        5. Config repo lost: `neo backup extract-config user@host:folder --key <admin key>` on a computer with the admin key

        ## Pitfalls
        - Two destinations may share a host, not a folder or dataset (assertion)
        - ZFS: each ZFS destination keeps its last received snapshot here as its incremental base (/var/lib/neo-backup/zfs-base-<name>)
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
