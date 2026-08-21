# Docker auto-updater implementation: scheduled pulls + restarts using the containers registry from lib/containers.nix.
#
# Images are often shared across services (e.g. redis:7 for paperless + activepieces).
# Pull once per unique image; if the image ID changes, restart every container that uses it.
# When Hermes superviseUpdates is on, tag the previous image and write a change manifest
# so Hermes can roll back a broken pull.
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
      supervise =
        (config.neo.services.hermes.enabled or false)
        && (config.neo.services.hermes.superviseUpdates or false);
      updaterStateDir = lib.neo.updaterStateDir;
      dockerManifest = lib.neo.dockerUpdaterManifest;

      # Group by image so a single pull can restart all consumers of that image.
      containersByImage = groupBy (c: c.image) containers;

      rollbackTagFor = image: "neo-rollback:${replaceStrings ["/" ":"] ["__" "__"] image}";

      updateScript = pkgs.writeShellScript "neo-docker-update" ''
        set -euo pipefail
        ${optionalString supervise ''
          mkdir -p ${updaterStateDir}
          updates_json='[]'
          changed=0
          failed=0
        ''}
        ${concatMapStringsSep "\n" (
            image: let
              cs = containersByImage.${image};
              consumers = concatMapStringsSep ", " (c: "${c.container} (${c.service})") cs;
              rollbackTag = rollbackTagFor image;
              unitsJson = builtins.toJSON (map (c: "docker-${c.container}") cs);
            in ''
              echo "[$(date -Iseconds)] Checking ${escapeShellArg image} (used by: ${consumers})..."
              old_id=$(${pkgs.docker}/bin/docker image inspect --format '{{.Id}}' ${escapeShellArg image} 2>/dev/null || echo "none")
              ${pkgs.docker}/bin/docker pull ${escapeShellArg image} || true
              new_id=$(${pkgs.docker}/bin/docker image inspect --format '{{.Id}}' ${escapeShellArg image} 2>/dev/null || echo "none")
              if [ "$old_id" != "$new_id" ] && [ "$new_id" != "none" ]; then
                echo "  Image updated ($old_id -> $new_id); restarting all consumers"
                ${optionalString supervise ''
                if [ "$old_id" != "none" ]; then
                  echo "  Tagging previous image as ${escapeShellArg rollbackTag}"
                  ${pkgs.docker}/bin/docker tag "$old_id" ${escapeShellArg rollbackTag} || true
                fi
                changed=1
                updates_json=$(${pkgs.jq}/bin/jq -c \
                  --arg image ${escapeShellArg image} \
                  --arg old "$old_id" \
                  --arg new "$new_id" \
                  --arg tag ${escapeShellArg rollbackTag} \
                  --argjson units ${escapeShellArg unitsJson} \
                  '. + [{image:$image, old_id:$old, new_id:$new, rollback_tag:$tag, units:$units}]' \
                  <<<"$updates_json")
              ''}
                ${concatMapStringsSep "\n" (c: ''
                  echo "    systemctl restart docker-${c.container} (${c.service})"
                  if ! ${pkgs.systemd}/bin/systemctl restart "docker-${c.container}"; then
                    echo "    restart failed: docker-${c.container}"
                    ${optionalString supervise "failed=1"}
                  fi
                '')
                cs}
              else
                echo "  Up to date or no change."
              fi
            ''
          )
          (attrNames containersByImage)}
        echo "Docker update check complete."
        ${optionalString supervise ''
          ${pkgs.jq}/bin/jq -n \
            --argjson changed "$([ "$changed" -eq 1 ] && echo true || echo false)" \
            --argjson failed "$([ "$failed" -eq 1 ] && echo true || echo false)" \
            --argjson updates "$updates_json" \
            --arg finished "$(date -Iseconds)" \
            '{kind:"docker",changed:$changed,failed:$failed,updates:$updates,finished_at:$finished}' \
            > ${dockerManifest}
          echo "Wrote ${dockerManifest} (changed=$changed failed=$failed)"
        ''}
      '';

      rollbackScript = pkgs.writeShellScriptBin "neo-docker-rollback" ''
        set -euo pipefail
        MANIFEST=${dockerManifest}
        DOCKER=${pkgs.docker}/bin/docker
        SYSTEMCTL=${pkgs.systemd}/bin/systemctl
        JQ=${pkgs.jq}/bin/jq

        usage() {
          echo "usage: neo-docker-rollback [--all] [--image IMAGE]" >&2
          exit 2
        }

        filter=""
        while [ $# -gt 0 ]; do
          case "$1" in
            --all) shift ;;
            --image)
              [ $# -ge 2 ] || usage
              filter=$2
              shift 2
              ;;
            -h|--help) usage ;;
            *) usage ;;
          esac
        done

        if [ ! -f "$MANIFEST" ]; then
          echo "neo-docker-rollback: missing $MANIFEST" >&2
          exit 1
        fi

        any=0
        fail=0
        while IFS= read -r row; do
          image=$($JQ -r .image <<<"$row")
          if [ -n "$filter" ] && [ "$image" != "$filter" ]; then
            continue
          fi
          old_id=$($JQ -r .old_id <<<"$row")
          if [ "$old_id" = "none" ] || [ -z "$old_id" ] || [ "$old_id" = "null" ]; then
            echo "skip $image: no previous image id"
            continue
          fi
          any=1
          echo "Rolling back $image to $old_id"
          $DOCKER tag "$old_id" "$image"
          while IFS= read -r unit; do
            [ -n "$unit" ] || continue
            echo "  systemctl restart $unit"
            if $SYSTEMCTL restart "$unit"; then
              if $SYSTEMCTL is-active --quiet "$unit"; then
                echo "  $unit active"
              else
                echo "  $unit not active after restart" >&2
                fail=1
              fi
            else
              echo "  restart failed: $unit" >&2
              fail=1
            fi
          done < <($JQ -r '.units[]' <<<"$row")
        done < <($JQ -c '.updates[]' "$MANIFEST")

        if [ "$any" -eq 0 ]; then
          echo "neo-docker-rollback: no matching updates in $MANIFEST" >&2
          exit 1
        fi
        exit "$fail"
      '';
    in {
      config = mkIf cfg.enabled {
        environment.systemPackages = optional supervise rollbackScript;

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
