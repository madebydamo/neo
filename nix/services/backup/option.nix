# Backup service options.
{...}: {
  flake.modules.nixos.backup-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.backup = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Backup: a copy of all your homeserver's data on another computer or disk, every night" {rank = 0;};

              # docs/design/backup-restore.md §2. neo restore reads the same keys.
              target = mkOption {
                type = types.enum ["ssh" "disk"];
                default = "ssh";
                rank = 5;
                description = "Where the backup goes: another computer you can log in to, or a disk attached to this server.";
                ui = lib.neo.ui.mkUi {
                  label = "Back up to";
                  choiceLabels = {
                    ssh = "Another computer (SSH)";
                    disk = "A disk on this server";
                  };
                };
              };

              format = mkOption {
                type = types.enum ["files" "zfs"];
                default = "files";
                rank = 7;
                description = "Plain files work with any computer or disk. ZFS snapshots are faster and keep several versions, but need ZFS on both sides.";
                ui = lib.neo.ui.mkUi {
                  label = "Backup format";
                  choiceLabels = {
                    files = "Plain files";
                    zfs = "ZFS snapshots";
                  };
                };
              };

              # Defaults to neo.core.volumes.root; on ZFS copied from a snapshot.
              sourceDir = mkOption {
                type = types.path;
                default = config.neo.core.volumes.root;
                description = "Folder on this homeserver that gets backed up. The default is the whole data volume (all app data). Plain files only.";
                rank = 50;
                ui = lib.neo.ui.mkUi {
                  label = "Folder to back up";
                  group = lib.neo.ui.groups.advanced;
                };
              };

              # files: rsync destination folder. Relative paths are relative to the
              # remote user's home directory (rsync user@host:path); a disk needs
              # an absolute path below its mount point.
              remotePath = mkOption {
                type = types.str;
                default = config.neo.core.hostname;
                description = "Folder where this homeserver's files are stored. On another computer, a name alone is a folder in that user's home; on a disk, give the full path where the disk is mounted.";
                rank = 60;
                ui = lib.neo.ui.mkUi {
                  label = "Backup folder";
                  visibleWhen = "format=files";
                };
              };

              # zfs: dataset receiving the snapshots (zfs recv -F).
              dataset = mkOption {
                type = types.str;
                default = "";
                example = "tank/backup/homeserver";
                description = "ZFS dataset that receives the backups, starting with the pool name, for example tank/backup/homeserver. It is created on the first backup.";
                rank = 61;
                ui = lib.neo.ui.mkUi {
                  label = "Backup dataset";
                  visibleWhen = "format=zfs";
                };
              };

              keep = mkOption {
                type = types.ints.positive;
                default = 14;
                description = "How many backups are kept at the destination. Older ones are deleted after each backup.";
                rank = 62;
                ui = lib.neo.ui.mkUi {
                  label = "Backups to keep";
                  visibleWhen = "format=zfs";
                };
              };

              # systemd OnCalendar expression.
              schedule = mkOption {
                type = types.str;
                default = "00:00:00";
                description = "Time of day the backup runs, e.g. 03:00:00 for 3 AM (default: midnight). Also accepts systemd calendar times such as Sun 02:00.";
                rank = 70;
                ui = lib.neo.ui.mkUi {label = "Backup time";};
              };

              # rsync --exclude patterns, relative to the transfer root (sourceDir).
              excludedDirs = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "Folders to leave out of the backup, as paths inside Folder to back up, e.g. DATA/AppData/jellyfin/cache or DATA/Media. A plain name such as cache skips every folder with that name. Plain files only.";
                rank = 80;
                ui = lib.neo.ui.mkUi {
                  label = "Skip folders";
                  visibleWhen = "format=files";
                };
              };

              logFile = mkOption {
                type = types.path;
                internal = true;
                default = "/var/log/backup.log";
                description = "Path to log file for backup operations";
              };
            }
            # The SSH host key by default (backup runs as root): it is escrowed in
            # identity.sops.yaml, so a restored machine and `neo restore` log in
            # with it too. Configs from before keep the homeserver key (migration
            # 013_backup_keeps_ssh_key).
            // (let
              ssh = lib.neo.mkSshConnectionOptions {
                rankBase = 10;
                defaultSshKey = "/etc/ssh/ssh_host_ed25519_key";
                hostDescription = "Name or IP address of the computer that receives the backup.";
                userDescription = "Username you log in with on that computer. For ZFS snapshots it needs the right to receive them (root, or zfs allow).";
                sshKeyDescription = "Key this server logs in with. The default is the server's own identity, which a restore brings back, so the backup keeps working after a restore. Add its public key (System → SSH) on the backup computer.";
                extraOptionsDescription = "Extra SSH options for the connection, e.g. -p 2222 for a different port.";
              };
              onSsh = ui: lib.neo.ui.mkUi (ui // {visibleWhen = "target=ssh";});
            in {
              host =
                ssh.host
                // {
                  default = "";
                  ui = onSsh {label = "Backup computer";};
                };
              user =
                ssh.user
                // {
                  default = "root";
                  ui = onSsh {label = "Username";};
                };
              sshKey =
                ssh.sshKey
                // {
                  ui = onSsh {
                    label = "SSH key";
                    group = lib.neo.ui.groups.advanced;
                  };
                };
              extraOptions =
                ssh.extraOptions
                // {
                  ui = onSsh {
                    label = "Extra SSH options";
                    group = lib.neo.ui.groups.advanced;
                  };
                };
            })
            // lib.neo.mkSystemdUnits ["backup"]
            // lib.neo.mkSystemdTimers ["backup"]
            // lib.neo.mkServiceMeta {
              category = "Files";
              icon = ./icon.svg;
              tagline = "Copies your data to another computer or disk every night";
              description = ''
                Keeps a copy of your app data and settings on another computer you control, for example a friend's server or a second box at your parents' place, or on a disk attached to this server.
                If this server breaks, `neo restore` installs a new one from your configuration and this backup.
              '';
              notes = ''
                Runs every night (Backup time). On the ZFS disk layout it copies from a snapshot, so databases are copied at one moment.
                Plain files: rsync over SSH or to a mounted disk. ZFS snapshots: zfs send, incremental after the first run; the user needs root or zfs allow create,receive,mount,destroy on the destination.
                The default SSH key is the server's host key: add its public key (System → SSH) to the backup computer's authorized_keys.
                Restore on a new machine: neo restore <config repo> --host root@<new machine> (docs/RESTORE.md).
              '';
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Backup of all Neo data to another computer or disk.";
      };
    };
}
