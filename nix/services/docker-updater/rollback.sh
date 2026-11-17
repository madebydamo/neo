# neo-docker-rollback: tag each image of the last docker update run back to
# its previous id and restart its containers (Hermes superviseUpdates).
#
# Env (set by default.nix):
#   NEO_MANIFEST  the run to roll back (default for --manifest)

MANIFEST=$NEO_MANIFEST

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
    -h | --help) usage ;;
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
  image=$(jq -r .image <<<"$row")
  if [ -n "$filter" ] && [ "$image" != "$filter" ]; then
    continue
  fi
  old_id=$(jq -r .old_id <<<"$row")
  if [ "$old_id" = "none" ] || [ -z "$old_id" ] || [ "$old_id" = "null" ]; then
    echo "skip $image: no previous image id"
    continue
  fi
  any=1
  echo "Rolling back $image to $old_id"
  docker tag "$old_id" "$image"
  while IFS= read -r unit; do
    [ -n "$unit" ] || continue
    echo "  systemctl restart $unit"
    if systemctl restart "$unit"; then
      if systemctl is-active --quiet "$unit"; then
        echo "  $unit active"
      else
        echo "  $unit not active after restart" >&2
        fail=1
      fi
    else
      echo "  restart failed: $unit" >&2
      fail=1
    fi
  done < <(jq -r '.units[]' <<<"$row")
done < <(jq -c '.updates[]' "$MANIFEST")

if [ "$any" -eq 0 ]; then
  echo "neo-docker-rollback: no matching updates in $MANIFEST" >&2
  exit 1
fi
exit "$fail"
