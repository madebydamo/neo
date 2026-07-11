# Backup service implementation.
{...}: {
  flake.modules.nixos.backup = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.backup;

      excludes = concatStringsSep " " (map (dir: "--exclude '${dir}'") cfg.excludedDirs);

      sshOptions = "-i ${escapeShellArg cfg.sshKey} ${concatStringsSep " " cfg.extraOptions}";

      backup-script = pkgs.writeShellScriptBin "backup-to-rsync" ''
        set -e

        SOURCE_DIR="${cfg.sourceDir}"
        DEST_DIR="${cfg.user}@${cfg.host}:${cfg.remotePath}"
        LOG_FILE="${cfg.logFile}"
        SSH_KEY="${cfg.sshKey}"

        if [ ! -f "$SSH_KEY" ]; then
          echo "Backup SSH key missing at $SSH_KEY (homeserver key is generated on activation)" >&2
          exit 1
        fi

        echo "Starting backup to ${cfg.host} at $(date)"

        if ${pkgs.rsync}/bin/rsync -avz --delete -e "${pkgs.openssh}/bin/ssh ${sshOptions}" ${excludes} "$SOURCE_DIR" "$DEST_DIR" > "$LOG_FILE" 2>&1; then
          echo "Backup completed successfully to ${cfg.host} at $(date)"
          echo "Backup details written to $LOG_FILE"
        else
          echo "Backup failed to ${cfg.host} at $(date)" >&2
          echo "Check $LOG_FILE for details" >&2
          exit 1
        fi
      '';
    in {
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
    };
}
