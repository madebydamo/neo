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

              sshKey = mkOption {
                type = types.path;
                description = "Path to SSH private key for authentication";
                rank = 10;
              };

              remoteServer = mkOption {
                type = types.str;
                description = "Remote backup server hostname or IP";
                rank = 20;
              };

              remoteUser = mkOption {
                type = types.str;
                description = "Username for SSH connection to backup server";
                rank = 30;
              };

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

              sshExtraOptions = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "Additional SSH options for the rsync connection";
                rank = 92;
              };
            }
            // lib.neo.mkSystemdUnits [
              "backup"
            ]
            // lib.neo.mkServiceMeta {
              icon = "💾";
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
