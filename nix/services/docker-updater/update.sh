# Docker auto-update run (default.nix): pull every image once, restart all
# containers that use it when its id changed, and record the run.
#
# Env (set by default.nix):
#   NEO_HISTORY_DIR  run history (<run>.json, <run>.log, last.json)
#   NEO_PLAN         JSON list, one entry per image:
#                    {image, consumers, rollback_tag, units, restarts: [{unit, service}]}
#   NEO_SUPERVISE    true: tag the previous image for neo-docker-rollback
#   NEO_LOCK_DIR     neo operation locks (cli/src/utils/locks.rs)

# Start of the run (the same in system-updater/auto-update.sh): run_id, history file and log,
# all further output also to the log. The web UI reads <run_id>.json/.log.
run_id=$(date -u +%Y%m%dT%H%M%SZ)-$$
started_at=$(date -Iseconds)
mkdir -p "$NEO_HISTORY_DIR"
hist=$NEO_HISTORY_DIR/$run_id.json
log_file=$NEO_HISTORY_DIR/$run_id.log
exec > >(tee -a "$log_file") 2>&1

updates_json='[]'
changed=0
failed=0
hist_done=0

write_hist() {
  local in_progress=$1
  local finished=$2
  jq -n \
    --arg run_id "$run_id" \
    --arg started "$started_at" \
    --argjson in_progress "$in_progress" \
    --argjson changed "$([ "$changed" -eq 1 ] && echo true || echo false)" \
    --argjson failed "$([ "$failed" -eq 1 ] && echo true || echo false)" \
    --argjson updates "$updates_json" \
    --arg finished "$finished" \
    --arg log "$run_id.log" \
    '{kind:"docker",run_id:$run_id,started_at:$started,in_progress:$in_progress,changed:$changed,failed:$failed,updates:$updates,finished_at:(if $finished == "" then null else $finished end),log:$log}' \
    >"$hist"
}

finalize() {
  [ "$hist_done" -eq 1 ] && return
  hist_done=1
  trap - EXIT INT TERM HUP
  if [ "${1:-}" = abort ]; then
    failed=1
  fi
  write_hist false "$(date -Iseconds)"
  echo "Wrote $hist (changed=$changed failed=$failed in_progress=false); last.json -> $run_id.json"
}

# Normalize inspect output: trim whitespace; missing image => none. Only
# content-addressed ids (sha256:…) are taggable; anything else is a first pull
# (empty, "none", or garbage like a leading newline + "none").
image_id() {
  local id
  id=$(docker image inspect --format '{{.Id}}' "$1" 2>/dev/null || true)
  id=$(printf '%s' "$id" | tr -d '[:space:]')
  case "$id" in
    sha256:* | sha512:*) echo "$id" ;;
    *) echo none ;;
  esac
}

# restart UNIT SERVICE
restart() {
  local unit=$1 service=$2 load_state rc
  # Require a loaded unit before restart. Optional sidecars (e.g. gated out of
  # systemdUnits / oci-containers) must not flip failed=true.
  load_state=$(systemctl show -p LoadState --value "$unit" 2>/dev/null || echo not-found)
  if [ "$load_state" != "loaded" ]; then
    echo "    skipped $unit ($service): unit not found (LoadState=$load_state)"
    return 0
  fi
  echo "    systemctl restart $unit ($service)"
  rc=0
  flock -E 75 -w 120 "$NEO_LOCK_DIR/unit_$unit.service.lock" \
    systemctl restart "$unit" || rc=$?
  if [ "$rc" -eq 75 ]; then
    echo "    skipped $unit: locked by a neo operation (restore / clear appdata / pull)"
  elif [ "$rc" -ne 0 ]; then
    echo "    restart failed: $unit"
    failed=1
  fi
}

write_hist true ""
ln -sfn "$run_id.json" "$NEO_HISTORY_DIR/last.json"
echo "Started run $run_id; last.json -> $run_id.json (in_progress)"
trap 'finalize abort' EXIT INT TERM HUP

while IFS= read -r entry; do
  image=$(jq -r .image <<<"$entry")
  echo "[$(date -Iseconds)] Checking $image (used by: $(jq -r .consumers <<<"$entry"))..."
  old_id=$(image_id "$image")
  docker pull "$image" || true
  new_id=$(image_id "$image")
  if [ "$old_id" != "$new_id" ] && [ "$new_id" != "none" ]; then
    echo "  Image updated ($old_id -> $new_id); restarting all consumers"
    rollback_tag=""
    if [ "$NEO_SUPERVISE" = true ]; then
      tag=$(jq -r .rollback_tag <<<"$entry")
      if [ "$old_id" != "none" ]; then
        echo "  Tagging previous image as $tag"
        docker tag "$old_id" "$tag" || true
        rollback_tag=$tag
      else
        echo "  First pull (no previous image id); skipping rollback tag"
      fi
    fi
    changed=1
    updates_json=$(jq -c \
      --arg image "$image" \
      --arg old "$old_id" \
      --arg new "$new_id" \
      --arg tag "$rollback_tag" \
      --argjson units "$(jq -c .units <<<"$entry")" \
      '. + [{image:$image, old_id:$old, new_id:$new, rollback_tag:$tag, units:$units}]' \
      <<<"$updates_json")
    while IFS=$'\t' read -r unit service; do
      restart "$unit" "$service"
    done < <(jq -r '.restarts[] | [.unit, .service] | @tsv' <<<"$entry")
  else
    echo "  Up to date or no change."
  fi
done < <(jq -c '.[]' "$NEO_PLAN")
echo "Docker update check complete."
finalize ok
