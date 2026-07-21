# Docker auto-updater implementation: scheduled pulls + restarts using the containers registry from lib/containers.nix.
#
# Images are often shared across services (e.g. redis:7 for paperless + activepieces).
# Pull once per unique image; if the image ID changes, restart every container that uses it.
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

      # Group by image so a single pull can restart all consumers of that image.
      containersByImage = groupBy (c: c.image) containers;

      updateScript = pkgs.writeShellScript "neo-docker-update" ''
        set -euo pipefail
        ${concatMapStringsSep "\n" (
            image: let
              cs = containersByImage.${image};
              consumers = concatMapStringsSep ", " (c: "${c.container} (${c.service})") cs;
            in ''
              echo "[$(date -Iseconds)] Checking ${escapeShellArg image} (used by: ${consumers})..."
              old_id=$(${pkgs.docker}/bin/docker image inspect --format '{{.Id}}' ${escapeShellArg image} 2>/dev/null || echo "none")
              ${pkgs.docker}/bin/docker pull ${escapeShellArg image} || true
              new_id=$(${pkgs.docker}/bin/docker image inspect --format '{{.Id}}' ${escapeShellArg image} 2>/dev/null || echo "none")
              if [ "$old_id" != "$new_id" ] && [ "$new_id" != "none" ]; then
                echo "  Image updated ($old_id -> $new_id); restarting all consumers"
                ${concatMapStringsSep "\n" (c: ''
                    echo "    systemctl restart docker-${c.container} (${c.service})"
                    ${pkgs.systemd}/bin/systemctl restart "docker-${c.container}" || true
                  '')
                  cs}
              else
                echo "  Up to date or no change."
              fi
            ''
          )
          (attrNames containersByImage)}
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
