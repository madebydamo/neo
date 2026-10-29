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

              # After SSH band (rankBase 10 → host/user/sshKey/extra = 10–40)
              sourceDir = mkOption {
                type = types.path;
                default = config.neo.core.volumes.root;
                description = "Source directory to backup";
                rank = 50;
              };

              remotePath = mkOption {
                type = types.str;
                default = config.neo.core.hostname;
                description = "Remote path on the backup server";
                rank = 60;
              };

              schedule = mkOption {
                type = types.str;
                default = "00:00:00";
                description = "Time to run the backup (OnCalendar format)";
                rank = 70;
              };

              excludedDirs = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "List of directories to exclude from backup";
                rank = 80;
              };

              logFile = mkOption {
                type = types.path;
                internal = true;
                default = "/var/log/backup.log";
                description = "Path to log file for backup operations";
              };
            }
            // lib.neo.mkSshConnectionOptions {
              rankBase = 10;
              hostDescription = "Remote backup server hostname or IP";
              userDescription = "Username for SSH connection to backup server";
              sshKeyDescription = "Path to SSH private key for rsync-over-SSH. Defaults to the auto-generated homeserver key (created at activation if missing). Root runs the backup so source files stay readable; only the key is shared with homeserver. Override only if you need a different key.";
              extraOptionsDescription = "Additional SSH options for the rsync connection";
            }
            // lib.neo.mkSystemdUnits ["backup"]
            // lib.neo.mkSystemdTimers ["backup"]
            // lib.neo.mkServiceMeta {
              category = "Files";
              icon = ./icon.svg;
              tagline = "Copies your data to another server every night";
              description = ''
                Keeps a copy of your app data and settings on another computer you control, for example a friend's server or a second box at your parents' place.
                If this server breaks, your photos, documents and settings are safe elsewhere.
              '';
              notes = ''
                Runs rsync over SSH on a schedule (systemd timer, midnight by default) to a remote server.
                Supports exclude lists, a log per run and extra SSH options for jump hosts or restricted keys.
              '';
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Rsync backup service configuration";
      };
    };
}
