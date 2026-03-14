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
          options = {
            enabled = mkEnableOption (mdDoc "Enable the rsync backup service");

            sourceDir = mkOption {
              type = types.path;
              default = "/var/neo";
              description = mdDoc "Source directory to backup";
            };

            remotePath = mkOption {
              type = types.str;
              description = mdDoc "Remote path on the backup server";
            };

            sshKey = mkOption {
              type = types.path;
              description = mdDoc "Path to SSH private key for authentication";
            };

            remoteServer = mkOption {
              type = types.str;
              description = mdDoc "Remote backup server hostname or IP";
            };

            remoteUser = mkOption {
              type = types.str;
              description = mdDoc "Username for SSH connection to backup server";
            };

            excludedDirs = mkOption {
              type = types.listOf types.str;
              default = [];
              description = mdDoc "List of directories to exclude from backup";
            };

            logFile = mkOption {
              type = types.path;
              default = "/var/log/backup.log";
              description = mdDoc "Path to log file for backup operations";
            };

            schedule = mkOption {
              type = types.str;
              default = "00:00:00";
              description = mdDoc "Time to run the backup (OnCalendar format)";
            };

            sshExtraOptions = mkOption {
              type = types.listOf types.str;
              default = [];
              description = mdDoc "Additional SSH options for the rsync connection";
            };
          };
        };
        default = {};
        description = mdDoc "Rsync backup service configuration";
      };
    };
}
