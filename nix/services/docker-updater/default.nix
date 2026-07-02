# Docker auto-updater implementation: scheduled pulls + restarts using the containers registry from lib/containers.nix.
{...}: {
  flake.modules.nixos.docker-updater = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services."docker-updater";
      containers = neo.getAllContainers config;

      updateScript = pkgs.writeShellScript "neo-docker-update" ''
        set -euo pipefail
        ${concatMapStringsSep "\n" (c: ''
            echo "[$(date -Iseconds)] Checking ${c.container} (${c.image}) for ${c.service}..."
            output=$(${pkgs.docker}/bin/docker pull ${escapeShellArg c.image} 2>&1 || true)
            if echo "$output" | grep -qE "(Downloaded newer image|Status: Downloaded newer image)"; then
              echo "  Newer image downloaded; restarting docker-${c.container}"
              ${pkgs.systemd}/bin/systemctl restart "docker-${c.container}" || true
            else
              echo "  Up to date or no change."
            fi
          '')
          containers}
        echo "Docker update check complete."
      '';
    in {
      config = mkIf cfg.enabled {
        systemd.services.neo-docker-updater = {
          description = "Pull updated Docker images for neo services and restart containers";
          serviceConfig = {
            Type = "oneshot";
            ExecStart = "${updateScript}";
            User = "root";
          };
        };

        systemd.timers.neo-docker-updater = {
          wantedBy = ["timers.target"];
          timerConfig = {
            OnCalendar = cfg.schedule;
            Persistent = true;
            RandomizedDelaySec = "30m";
            AccuracySec = "1h";
            Unit = "neo-docker-updater.service";
          };
        };
      };
    };
}
