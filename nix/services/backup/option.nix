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
              enabled = mkEnableOption "Enable the rsync backup service" {rank = 0;};

              sourceDir = mkOption {
                type = types.path;
                default = config.neo.core.volumes.root;
                description = "Source directory to backup";
                rank = 40;
              };

              remotePath = mkOption {
                type = types.str;
                default = config.neo.core.hostname;
                description = "Remote path on the backup server";
                rank = 50;
              };

              schedule = mkOption {
                type = types.str;
                default = "00:00:00";
                description = "Time to run the backup (OnCalendar format)";
                rank = 60;
              };

              excludedDirs = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "List of directories to exclude from backup";
                rank = 90;
              };

              logFile = mkOption {
                type = types.path;
                internal = true;
                default = "/var/log/backup.log";
                description = "Path to log file for backup operations";
                rank = 91;
              };
            }
            // lib.neo.mkSshConnectionOptions {
              hostRank = 20;
              userRank = 30;
              sshKeyRank = 10;
              extraOptionsRank = 92;
              hostDescription = "Remote backup server hostname or IP";
              userDescription = "Username for SSH connection to backup server";
              sshKeyDescription = "Path to SSH private key for rsync-over-SSH. Defaults to the auto-generated homeserver key (created at activation if missing). Root runs the backup so source files stay readable; only the key is shared with homeserver. Override only if you need a different key.";
              extraOptionsDescription = "Additional SSH options for the rsync connection";
            }
            // lib.neo.mkSystemdUnits ["backup"]
            // lib.neo.mkServiceMeta {
              category = "Files";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/borg.svg";
              description = ''
                The backup service performs automated rsync-over-SSH snapshots of your critical Neo homeserver data (AppData, configs, etc.) to a remote server of your choice.
                It supports custom exclude lists, scheduled execution via systemd OnCalendar timers, detailed per-run logging, and extra SSH options for bastions or restricted keys.
                A simple, reliable off-site backup solution that stays entirely under your control without third-party SaaS or cloud storage.
              '';
            };
        };
        default = {};
        description = "Rsync backup service configuration";
      };
    };
}
