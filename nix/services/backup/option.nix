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
              enabled = mkEnableOption "Off-site backup: a daily copy of your homeserver's data on another server" {rank = 0;};

              # After SSH band (rankBase 10 → host/user/sshKey/extra = 10–40)
              # Defaults to neo.core.volumes.root; copied with rsync over SSH.
              sourceDir = mkOption {
                type = types.path;
                default = config.neo.core.volumes.root;
                description = "Folder on this homeserver that gets backed up. The default is the whole data volume (all app data).";
                rank = 50;
                ui = lib.neo.ui.mkUi {
                  label = "Folder to back up";
                  group = lib.neo.ui.groups.advanced;
                };
              };

              # Defaults to neo.core.hostname. Relative paths are relative to the
              # remote user's home directory (rsync user@host:path).
              remotePath = mkOption {
                type = types.str;
                default = config.neo.core.hostname;
                description = "Folder on the backup server where this homeserver's files are stored. The default is a folder named after this homeserver.";
                rank = 60;
                ui = lib.neo.ui.mkUi {label = "Folder on backup server";};
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
                description = "Folders to leave out of the backup, e.g. appdata/jellyfin/cache (inside Folder to back up). A plain name such as cache skips every folder with that name.";
                rank = 80;
                ui = lib.neo.ui.mkUi {label = "Skip folders";};
              };

              logFile = mkOption {
                type = types.path;
                internal = true;
                default = "/var/log/backup.log";
                description = "Path to log file for backup operations";
              };
            }
            # SSH key defaults to the auto-generated homeserver key (created at activation
            # if missing). Root runs the backup so source files stay readable; only the
            # key is shared with homeserver.
            // (let
              ssh = lib.neo.mkSshConnectionOptions {
                rankBase = 10;
                hostDescription = "Name or IP address of the server that receives the backup.";
                userDescription = "Username you log in with on the backup server.";
                sshKeyDescription = "Key file used to log in to the backup server. Leave the default unless you were given a different key.";
                extraOptionsDescription = "Extra SSH options for the connection, e.g. -p 2222 for a different port.";
              };
            in
              ssh
              // {
                host = ssh.host // {ui = lib.neo.ui.mkUi {label = "Backup server";};};
                user = ssh.user // {ui = lib.neo.ui.mkUi {label = "Username";};};
                sshKey =
                  ssh.sshKey
                  // {
                    ui = lib.neo.ui.mkUi {
                      label = "SSH key";
                      group = lib.neo.ui.groups.advanced;
                    };
                  };
                extraOptions =
                  ssh.extraOptions
                  // {
                    ui = lib.neo.ui.mkUi {
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
