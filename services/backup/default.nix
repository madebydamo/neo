{
  config,
  lib,
  pkgs,
  ...
}:
with lib; let
  cfg = config.neo.services.backup;

  excludes = concatStringsSep " " (map (dir: "--exclude '${dir}'") cfg.excludedDirs);

  sshOptions = ''-i ${cfg.sshKey} ${concatStringsSep " " cfg.sshExtraOptions}'';

  backup-script = pkgs.writeShellScriptBin "backup-to-rsync" ''
    set -e

    SOURCE_DIR="${cfg.sourceDir}"
    DEST_DIR="${cfg.remoteUser}@${cfg.remoteServer}:${cfg.remotePath}"
    LOG_FILE="${cfg.logFile}"

    echo "Starting backup to ${cfg.remoteServer} at $(date)"

    if ${pkgs.rsync}/bin/rsync -avz --delete -e "${pkgs.openssh}/bin/ssh ${sshOptions}" ${excludes} "$SOURCE_DIR" "$DEST_DIR" > "$LOG_FILE" 2>&1; then
      echo "Backup completed successfully to ${cfg.remoteServer} at $(date)"
      echo "Backup details written to $LOG_FILE"
    else
      echo "Backup failed to ${cfg.remoteServer} at $(date)" >&2
      echo "Check $LOG_FILE for details" >&2
      exit 1
    fi
  '';
in {
  imports = [
    ./option.nix
  ];

  config = mkIf cfg.enabled {
    systemd.services.backup = {
      description = "Rsync backup to remote server";
      serviceConfig = {
        Type = "oneshot";
        ExecStart = "${backup-script}/bin/backup-to-rsync";
      };
    };

    systemd.timers.backup = {
      wantedBy = ["timers.target"];
      partOf = ["backup.service"];
      timerConfig = {
        OnCalendar = cfg.schedule;
        Unit = "backup.service";
      };
    };
  };
}
