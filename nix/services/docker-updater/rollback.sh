# neo-docker-rollback: point image references back at an older version from
# the image ledger and restart their containers
# (docs/design/docker-image-retention.md). Settings are never changed: the next
# scheduled update pulls the tag again and moves forward.
#
#   --previous  the version before the current one (default). Without --image
#               or --unit: every image the last update run changed (Hermes
#               superviseUpdates).
#   --at TIME   the version that ran at TIME (RFC 3339 or seconds since the
#               epoch), e.g. when a restored snapshot was taken. Without
#               --image or --unit: every image.
#   --no-restart  retag only (the caller starts the units, e.g. a restore).
#
# A version that is no longer on disk is pulled again by its digest; one that
# cannot be pulled (no digest, deleted from the registry) is skipped and
# reported, and the exit status is 1.
#
# Env (set by default.nix):
#   NEO_CLI          neo (docker-ledger)
#   NEO_PLAN         the updater plan (units per image)
#   NEO_LEDGER       image ledger
#   NEO_HISTORY_DIR  run history: last.json (last run), images.json (fallback ledger)
#   NEO_LOCK_DIR     neo operation locks
#   NEO_GC_OLDER_THAN  updater GC age ("" = none): moves the neo-history tags

usage() {
  echo "usage: neo-docker-rollback [--previous | --at TIME] [--image REF]... [--unit UNIT]... [--no-restart]" >&2
  exit 2
}

mode="previous"
at=""
restart=1
filters=()
while [ $# -gt 0 ]; do
  case "$1" in
    --previous | --all)
      mode="previous"
      shift
      ;;
    --at)
      [ $# -ge 2 ] || usage
      mode="at"
      at=$2
      shift 2
      ;;
    --image | --unit)
      [ $# -ge 2 ] || usage
      filters+=("$1" "$2")
      shift 2
      ;;
    --no-restart)
      restart=0
      shift
      ;;
    -h | --help) usage ;;
    *) usage ;;
  esac
done

select_args=(--ledger "$NEO_LEDGER" --fallback "$NEO_HISTORY_DIR/images.json")
if [ "$mode" = at ]; then
  select_args+=(--at "$at")
else
  select_args+=(--previous)
  if [ "${#filters[@]}" -eq 0 ]; then
    # Images the last run changed.
    last=$NEO_HISTORY_DIR/last.json
    if [ ! -f "$last" ]; then
      echo "neo-docker-rollback: no update run recorded ($last)" >&2
      exit 1
    fi
    while IFS= read -r img; do
      [ -n "$img" ] && filters+=(--image "$img")
    done < <(jq -r '.updates[]?.image' "$last")
    if [ "${#filters[@]}" -eq 0 ]; then
      echo "neo-docker-rollback: the last update run changed no image" >&2
      exit 1
    fi
  fi
fi
select_args+=("${filters[@]}")

picks=$("$NEO_CLI" docker-ledger select "${select_args[@]}")
if [ -z "$picks" ]; then
  echo "Nothing to roll back: every selected app already runs that version."
  exit 0
fi

fail=0
units=()
while IFS=$'\t' read -r ref id digest _local unit_list; do
  echo "Returning $ref to ${id:7:12}"
  if docker image inspect "$id" >/dev/null 2>&1; then
    docker tag "$id" "$ref" || {
      echo "  tagging failed" >&2
      fail=1
      continue
    }
  elif [ "$digest" != "-" ]; then
    echo "  not on disk any more; downloading $digest"
    if docker pull "$digest" && docker tag "$digest" "$ref"; then
      :
    else
      echo "  skipped $ref: could not download $digest (deleted from the registry?)" >&2
      fail=1
      continue
    fi
  else
    echo "  skipped $ref: not on disk any more and not downloadable (local image)" >&2
    fail=1
    continue
  fi
  if [ "$unit_list" != "-" ]; then
    IFS=, read -r -a us <<<"$unit_list"
    units+=("${us[@]}")
  fi
done <<<"$picks"

# The ledger records the old version as running from now on (and the
# neo-history tags follow, with a GC age).
age=()
[ -n "$NEO_GC_OLDER_THAN" ] && age=(--older-than "$NEO_GC_OLDER_THAN")
"$NEO_CLI" docker-ledger reconcile --plan "$NEO_PLAN" --ledger "$NEO_LEDGER" "${age[@]}" ||
  echo "warning: could not update the image ledger" >&2

if [ "$restart" -eq 0 ] || [ "${#units[@]}" -eq 0 ]; then
  exit "$fail"
fi

while IFS= read -r unit; do
  [ -n "$unit" ] || continue
  load_state=$(systemctl show -p LoadState --value "$unit" 2>/dev/null || echo not-found)
  if [ "$load_state" != loaded ]; then
    echo "  skipped $unit: unit not found"
    continue
  fi
  echo "  systemctl restart $unit"
  rc=0
  flock -E 75 -w 120 "$NEO_LOCK_DIR/unit_$unit.service.lock" \
    systemctl restart "$unit" || rc=$?
  if [ "$rc" -eq 75 ]; then
    echo "  skipped $unit: locked by a neo operation (restore / clear appdata / pull)" >&2
    fail=1
  elif [ "$rc" -ne 0 ]; then
    echo "  restart failed: $unit" >&2
    fail=1
  elif systemctl is-active --quiet "$unit"; then
    echo "  $unit active"
  else
    echo "  $unit not active after restart" >&2
    fail=1
  fi
done < <(printf '%s\n' "${units[@]}" | sort -u)
exit "$fail"
