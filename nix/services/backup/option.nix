# Backup service options.
{...}: {
  flake.modules.nixos.backup-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.backup = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Enable the rsync backup service";

              sourceDir = mkOption {
                type = types.path;
                default = "/var/neo";
                description = "Source directory to backup";
              };

              remotePath = mkOption {
                type = types.str;
                description = "Remote path on the backup server";
              };

              sshKey = mkOption {
                type = types.path;
                description = "Path to SSH private key for authentication";
              };

              remoteServer = mkOption {
                type = types.str;
                description = "Remote backup server hostname or IP";
              };

              remoteUser = mkOption {
                type = types.str;
                description = "Username for SSH connection to backup server";
              };

              excludedDirs = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "List of directories to exclude from backup";
              };

              logFile = mkOption {
                type = types.path;
                default = "/var/log/backup.log";
                description = "Path to log file for backup operations";
              };

              schedule = mkOption {
                type = types.str;
                default = "00:00:00";
                description = "Time to run the backup (OnCalendar format)";
              };

              sshExtraOptions = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "Additional SSH options for the rsync connection";
              };
            }
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
