# Docker auto-updater implementation: scheduled pulls + restarts using the containers registry from lib/containers.nix.
#
# Images are often shared across services (e.g. redis:7 for paperless + activepieces).
# Pull once per unique image; if the image ID changes, restart every container that uses it.
# Every run appends JSON + log under this service's appdata (updater/docker/).
# last.json is retargeted to an in-progress stub at start, then rewritten when
# the run finishes (or on SIGTERM). When Hermes superviseUpdates is on, also
# tag the previous image so Hermes can roll back a broken pull.
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
      updaterPaths = lib.neo.mkUpdaterPaths config.neo.core.volumes.appdata;
      dockerHistoryDir = cfg.appdata;
      dockerManifest = "${cfg.appdata}/last.json";

      # Group by image so a single pull can restart all consumers of that image.
      containersByImage = groupBy (c: c.image) containers;

      rollbackTagFor = image: "neo-rollback:${replaceStrings ["/" ":"] ["__" "__"] image}";

      updateScript = pkgs.writeShellScript "neo-docker-update" ''
        set -euo pipefail
        JQ=${pkgs.jq}/bin/jq
        run_id=$(date -u +%Y%m%dT%H%M%SZ)-$$
        started_at=$(date -Iseconds)
        mkdir -p ${dockerHistoryDir}
        hist=${dockerHistoryDir}/$run_id.json
        log_file=${dockerHistoryDir}/$run_id.log
        exec > >(tee -a "$log_file") 2>&1
        updates_json='[]'
        changed=0
        failed=0
        hist_done=0

        write_hist() {
          local in_progress=$1
          local finished=$2
          $JQ -n \
            --arg run_id "$run_id" \
            --arg started "$started_at" \
            --argjson in_progress "$in_progress" \
            --argjson changed "$([ "$changed" -eq 1 ] && echo true || echo false)" \
            --argjson failed "$([ "$failed" -eq 1 ] && echo true || echo false)" \
            --argjson updates "$updates_json" \
            --arg finished "$finished" \
            --arg log "$run_id.log" \
            '{kind:"docker",run_id:$run_id,started_at:$started,in_progress:$in_progress,changed:$changed,failed:$failed,updates:$updates,finished_at:(if $finished == "" then null else $finished end),log:$log}' \
            > "$hist"
        }

        finalize() {
          [ "$hist_done" -eq 1 ] && return
          hist_done=1
          trap - EXIT INT TERM HUP
          if [ "''${1:-}" = abort ]; then
            failed=1
          fi
          write_hist false "$(date -Iseconds)"
          echo "Wrote $hist (changed=$changed failed=$failed in_progress=false); last.json -> $run_id.json"
        }

        write_hist true ""
        ln -sfn "$run_id.json" ${dockerHistoryDir}/last.json
        echo "Started run $run_id; last.json -> $run_id.json (in_progress)"
        trap 'finalize abort' EXIT INT TERM HUP

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
                rollback_tag=""
                ${optionalString supervise ''
                if [ "$old_id" != "none" ]; then
                  echo "  Tagging previous image as ${escapeShellArg rollbackTag}"
                  ${pkgs.docker}/bin/docker tag "$old_id" ${escapeShellArg rollbackTag} || true
                  rollback_tag=${escapeShellArg rollbackTag}
                fi
              ''}
                changed=1
                updates_json=$(${pkgs.jq}/bin/jq -c \
                  --arg image ${escapeShellArg image} \
                  --arg old "$old_id" \
                  --arg new "$new_id" \
                  --arg tag "$rollback_tag" \
                  --argjson units ${escapeShellArg unitsJson} \
                  '. + [{image:$image, old_id:$old, new_id:$new, rollback_tag:$tag, units:$units}]' \
                  <<<"$updates_json")
                ${concatMapStringsSep "\n" (c: ''
                  echo "    systemctl restart docker-${c.container} (${c.service})"
                  if ! ${pkgs.systemd}/bin/systemctl restart "docker-${c.container}"; then
                    echo "    restart failed: docker-${c.container}"
                    failed=1
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
        finalize ok
      '';

      rollbackScript = pkgs.writeShellScriptBin "neo-docker-rollback" ''
        set -euo pipefail
        MANIFEST=${dockerManifest}
        DOCKER=${pkgs.docker}/bin/docker
        SYSTEMCTL=${pkgs.systemd}/bin/systemctl
        JQ=${pkgs.jq}/bin/jq

        usage() {
          echo "usage: neo-docker-rollback [--all] [--image IMAGE] [--manifest FILE]" >&2
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
            --manifest)
              [ $# -ge 2 ] || usage
              MANIFEST=$2
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

        system.activationScripts.neo-docker-updater-history = lib.neo.mkEnsureDirs config [
          {
            dirPath = updaterPaths.stateDir;
            mode = "0775";
            user = "homeserver";
            group = "homeserver";
          }
          {
            dirPath = dockerHistoryDir;
            mode = "0775";
            user = "homeserver";
            group = "homeserver";
          }
        ];

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
