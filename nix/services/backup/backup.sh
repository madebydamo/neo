# Neo backup (docs/design/backup-restore.md §2). Runs as root from backup.service.
#
# Env (set by default.nix):
#   NEO_TARGET       ssh | disk
#   NEO_FORMAT       files | zfs
#   NEO_SOURCE_DIR   folder to back up (files)
#   NEO_ROOT         neo.core.volumes.root, mountpoint of NEO_DATASET
#   NEO_DATASET      local ZFS dataset holding NEO_ROOT ("" without Disko)
#   NEO_DEST         remotePath (files)
#   NEO_DEST_DATASET dataset (zfs)
#   NEO_KEEP         backup snapshots kept at the destination (zfs)
#   NEO_SSH_DEST     user@host (ssh)
#   NEO_SSH          ssh command line with key and options (ssh)
#   NEO_EXCLUDES     newline-separated rsync excludes (files)
#   NEO_LOG          rsync log file

PREFIX=neo-backup-
snap=""

log() { echo "backup: $*"; }
die() {
  echo "backup: $*" >&2
  exit 1
}

# Commands at the destination: over SSH, or here for a disk.
read -r -a ssh_cmd <<<"${NEO_SSH:-}"
at_dest() {
  if [ "$NEO_TARGET" = ssh ]; then
    "${ssh_cmd[@]}" "$NEO_SSH_DEST" "$(printf '%q ' "$@")"
  else
    "$@"
  fi
}

cleanup() {
  if [ -n "$snap" ] && [ "$NEO_FORMAT" = files ]; then
    zfs destroy "$NEO_DATASET@$snap" 2>/dev/null || true
  fi
}
trap cleanup EXIT

take_snapshot() {
  snap="$PREFIX$(date -u +%Y%m%d-%H%M%S)"
  # A second run within the same second (manual start right after the timer).
  if zfs list -H "$NEO_DATASET@$snap" >/dev/null 2>&1; then
    sleep 1
    snap="$PREFIX$(date -u +%Y%m%d-%H%M%S)"
  fi
  zfs snapshot "$NEO_DATASET@$snap"
  log "snapshot $NEO_DATASET@$snap"
}

backup_files() {
  local src="$NEO_SOURCE_DIR" name dest
  name=$(basename "$NEO_SOURCE_DIR")
  if [ -n "$NEO_DATASET" ]; then
    take_snapshot
    # Same folder, frozen: <root>/.zfs/snapshot/<snap>/<path below root>.
    local rel="${NEO_SOURCE_DIR#"$NEO_ROOT"}"
    src="$NEO_ROOT/.zfs/snapshot/$snap$rel"
  fi
  if [ "$NEO_TARGET" = disk ]; then
    case "$NEO_DEST" in
    /*) ;;
    *) die "on a disk, the backup folder must be an absolute path ($NEO_DEST)" ;;
    esac
    mkdir -p "$NEO_DEST"
    if [ "$(findmnt -nvo TARGET --target "$NEO_DEST")" = / ]; then
      die "$NEO_DEST is on the system disk: is the backup disk mounted?"
    fi
    dest="$NEO_DEST/$name/"
  else
    dest="$NEO_SSH_DEST:${NEO_DEST:+$NEO_DEST/}$name/"
  fi

  local args=(-aH --numeric-ids --delete --mkpath --chmod=Du+w --info=stats2)
  [ "$NEO_TARGET" = ssh ] && args+=(-z -e "$NEO_SSH")
  while IFS= read -r e; do
    [ -n "$e" ] && args+=(--exclude "$e")
  done <<<"${NEO_EXCLUDES:-}"

  log "copying $NEO_SOURCE_DIR to $dest"
  if rsync "${args[@]}" "$src/" "$dest" >"$NEO_LOG" 2>&1 || unstick_and_retry "${args[@]}" "$src/" "$dest"; then
    tail -n 20 "$NEO_LOG"
    log "done"
  else
    tail -n 40 "$NEO_LOG" >&2
    die "rsync failed, see $NEO_LOG"
  fi
}

# Remotes holding a read-only dir (e.g. copied out of /nix/store) that later
# became a symlink or file: rsync cannot delete inside it. Make it writable and
# retry once. Logged paths are relative to the destination folder.
unstick_and_retry() {
  local stuck=() d base
  base=${NEO_DEST:+$NEO_DEST/}$(basename "$NEO_SOURCE_DIR")
  while IFS= read -r d; do
    [ -n "$d" ] && stuck+=("$base/$d")
  done < <(sed -n 's/^rsync: \[generator\] delete_file: unlink(\(.*\)) failed: Permission denied (13)$/\1/p' "$NEO_LOG" |
    while IFS= read -r f; do dirname -- "$f"; done | sort -u)
  [ "${#stuck[@]}" -gt 0 ] || return 1
  log "making read-only folders at the destination writable: ${stuck[*]}"
  if [ "$NEO_TARGET" = disk ]; then
    (cd / && chmod u+w "${stuck[@]}")
  else
    at_dest chmod u+w "${stuck[@]}"
  fi
  rsync "$@" >>"$NEO_LOG" 2>&1
}

backup_zfs() {
  [ -n "$NEO_DATASET" ] || die "ZFS format needs the ZFS disk layout (Disko) on this server"
  [ -n "$NEO_DEST_DATASET" ] || die "set the destination dataset for the ZFS format"
  local pool=${NEO_DEST_DATASET%%/*} imported=0
  if [ "$NEO_TARGET" = disk ] && ! zpool list -H "$pool" >/dev/null 2>&1; then
    zpool import -N "$pool" || die "pool $pool not found: is the backup disk attached?"
    imported=1
  fi

  # Newest snapshot both sides have: base of the incremental send.
  local remote base="" s
  if remote=$(at_dest zfs list -H -t snapshot -o name -s createtxg -d 1 "$NEO_DEST_DATASET" 2>/dev/null); then
    remote=$(printf '%s\n' "$remote" | sed -n "s|^.*@\($PREFIX.*\)\$|\1|p")
    for s in $(zfs list -H -t snapshot -o name -S createtxg -d 1 "$NEO_DATASET" | sed -n "s|^.*@\($PREFIX.*\)\$|\1|p"); do
      if printf '%s\n' "$remote" | grep -qx "$s"; then
        base=$s
        break
      fi
    done
    [ -n "$base" ] || die "$NEO_DEST_DATASET exists but shares no backup snapshot with $NEO_DATASET; refusing to overwrite it (rename or destroy it at the destination for a fresh full backup)"
  else
    at_dest zfs list -H -o name "$pool" >/dev/null 2>&1 || die "pool $pool not found at the destination"
  fi

  take_snapshot
  local send=(zfs send) recv=(zfs recv -u -F -o canmount=noauto -o com.sun:auto-snapshot=false "$NEO_DEST_DATASET")
  if [ -n "$base" ]; then
    send+=(-i "@$base")
    log "incremental send $base → $snap to $NEO_DEST_DATASET"
  else
    log "full send $snap to $NEO_DEST_DATASET (first backup)"
    # Parents of the destination dataset.
    local parent=${NEO_DEST_DATASET%/*}
    [ "$parent" = "$NEO_DEST_DATASET" ] || at_dest zfs create -p -o canmount=off "$parent" 2>/dev/null || true
  fi
  send+=("$NEO_DATASET@$snap")

  if [ "$NEO_TARGET" = ssh ]; then
    "${send[@]}" | "${ssh_cmd[@]}" "$NEO_SSH_DEST" "$(printf '%q ' "${recv[@]}")" || die "zfs send/recv failed"
  else
    "${send[@]}" | "${recv[@]}" || die "zfs send/recv failed"
  fi

  # Here: only the newest backup snapshot (next base). There: the newest NEO_KEEP.
  for s in $(zfs list -H -t snapshot -o name -S createtxg -d 1 "$NEO_DATASET" | grep "@$PREFIX" | tail -n +2); do
    zfs destroy "$s" || true
  done
  for s in $(at_dest zfs list -H -t snapshot -o name -S createtxg -d 1 "$NEO_DEST_DATASET" | grep "@$PREFIX" | tail -n "+$((NEO_KEEP + 1))"); do
    at_dest zfs destroy "$s" || true
  done
  [ "$imported" = 0 ] || zpool export "$pool" || true
  log "done"
}

log "starting ($NEO_FORMAT to $NEO_TARGET) at $(date)"
if [ "$NEO_TARGET" = ssh ]; then
  at_dest true || die "cannot log in to $NEO_SSH_DEST"
fi
case "$NEO_FORMAT" in
files) backup_files ;;
zfs) backup_zfs ;;
*) die "unknown format $NEO_FORMAT" ;;
esac
