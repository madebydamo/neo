# Neo backup (docs/design/backup-restore.md §2). Runs as root from backup.service:
# one snapshot, then a copy of it to every destination in turn. A destination
# that fails is reported and the next one still runs.
#
# Env (set by default.nix):
#   NEO_DESTINATIONS newline-separated name=file, in run order; each file sets
#                    the destination's variables below
#   NEO_ZFS_ACTIVE   newline-separated names of the enabled zfs destinations
#   NEO_ZFS_ALL      the same, disabled ones included (their base snapshot stays)
#   NEO_SOURCE_DIR   folder to back up (files, encrypted)
#   NEO_ROOT         neo.core.volumes.root, mountpoint of NEO_DATASET
#   NEO_DATASET      local ZFS dataset holding NEO_ROOT ("" without Disko)
#   NEO_EXCLUDES     newline-separated excludes below NEO_SOURCE_DIR (files, encrypted)
#   NEO_HOSTNAME     restic snapshot host (encrypted)
#   NEO_HOST_KEY     SSH host key: opens neo-backup.age, and a recipient of it (encrypted)
#   NEO_RECIPIENTS   newline-separated admin ssh-ed25519 keys, recipients of neo-backup.age (encrypted)
#   NEO_ROTATE       true: new data key and repository for every encrypted
#                    destination, the others are skipped (backup-rotate-key.service)
#
# Per destination:
#   NEO_NAME         its name in destinations.<name>
#   NEO_TARGET       ssh | disk
#   NEO_FORMAT       files | encrypted | zfs
#   NEO_DEST         remotePath (files, encrypted)
#   NEO_DEST_DATASET dataset (zfs)
#   NEO_KEEP         backups kept at the destination (zfs, encrypted)
#   NEO_SSH_DEST     user@host (ssh)
#   NEO_SSH          ssh command line with key and options (ssh)
#   NEO_LOG          rsync / restic log file

PREFIX=neo-backup-
snap=""
pruned=0
state_dir=${STATE_DIRECTORY:-/var/lib/neo-backup}

log() { echo "backup${NEO_NAME:+ [$NEO_NAME]}: $*"; }
die() {
  echo "backup${NEO_NAME:+ [$NEO_NAME]}: $*" >&2
  exit 1
}

# Commands at the destination: over SSH, or here for a disk.
at_dest() {
  if [ "$NEO_TARGET" = ssh ]; then
    "${ssh_cmd[@]}" "$NEO_SSH_DEST" "$(printf '%q ' "$@")"
  else
    "$@"
  fi
}

# An interrupted run: the snapshot goes unless a ZFS destination got it.
cleanup() {
  [ "$pruned" = 0 ] && [ -n "$snap" ] || return 0
  grep -qsx "$snap" "$state_dir"/zfs-base-* || zfs destroy "$NEO_DATASET@$snap" 2>/dev/null || true
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

# Folder to copy: on ZFS the same folder frozen in this run's snapshot,
# <root>/.zfs/snapshot/<snap>/<path below root>. Sets src.
source_folder() {
  src="$NEO_SOURCE_DIR"
  if [ -n "$snap" ]; then
    local rel="${NEO_SOURCE_DIR#"$NEO_ROOT"}"
    src="$NEO_ROOT/.zfs/snapshot/$snap$rel"
  fi
}

# A disk destination must be a mounted disk, not the system disk.
check_disk_dest() {
  case "$NEO_DEST" in
  /*) ;;
  *) die "on a disk, the backup folder must be an absolute path ($NEO_DEST)" ;;
  esac
  mkdir -p "$NEO_DEST"
  if [ "$(findmnt -nvo TARGET --target "$NEO_DEST")" = / ]; then
    die "$NEO_DEST is on the system disk: is the backup disk mounted?"
  fi
}

backup_files() {
  local src name dest
  name=$(basename "$NEO_SOURCE_DIR")
  source_folder
  if [ "$NEO_TARGET" = disk ]; then
    check_disk_dest
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
  [ -n "$snap" ] || die "ZFS format needs the ZFS disk layout (Disko) on this server"
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

  # The next base of this destination (prune_snapshots keeps it here).
  echo "$snap" >"$state_dir/zfs-base-$NEO_NAME"
  # There: the newest NEO_KEEP.
  for s in $(at_dest zfs list -H -t snapshot -o name -S createtxg -d 1 "$NEO_DEST_DATASET" | grep "@$PREFIX" | tail -n "+$((NEO_KEEP + 1))"); do
    at_dest zfs destroy "$s" || true
  done
  [ "$imported" = 0 ] || zpool export "$pool" || true
  log "done"
}

# --- encrypted: restic + neo-backup.age (docs/design/backup-restore.md §3) ---
#
# At the destination, in the backup folder:
#   neo-backup.age   the data key, age-encrypted to the host key and every admin
#                    ssh-ed25519 key (re-encrypted on every run, so key changes
#                    in System → SSH apply at the next backup)
#   restic-<UTC>/    the restic repository the data key opens
# Decrypted: lines version=1, format=restic, repository=restic-<UTC>,
# password=<random>. Rotation makes a new key and repository, and removes the
# old one only after the new header is in place.
HEADER=neo-backup.age
run_dir=${RUNTIME_DIRECTORY:-/run/backup}

# The backup folder, as sftp and restic see it ("." = the SSH user's home).
enc_dir() {
  local d=${NEO_DEST%/}
  echo "${d:-.}"
}

# sftp with the backup's ssh command (its options are ssh's, e.g. -p 2222).
sftp_batch() {
  sftp -q -S "$run_dir/ssh" -b - "$NEO_SSH_DEST"
}

ssh_wrapper() {
  printf '#!/bin/sh\nexec %s "$@"\n' "$NEO_SSH" >"$run_dir/ssh"
  chmod 700 "$run_dir/ssh"
}

ensure_enc_dir() {
  if [ "$NEO_TARGET" = disk ]; then
    check_disk_dest
    return
  fi
  local d path="" part
  d=$(enc_dir)
  [ "$d" = . ] && return
  case "$d" in /*) path=/ ;; esac
  # sftp's mkdir has no -p.
  local cmds=""
  IFS=/ read -r -a parts <<<"${d#/}"
  for part in "${parts[@]}"; do
    [ -n "$part" ] || continue
    path="$path$part"
    cmds+="-mkdir \"$path\""$'\n'
    path="$path/"
  done
  printf '%s' "$cmds" | sftp_batch >/dev/null || die "cannot reach $NEO_SSH_DEST over SFTP"
}

# Download the header to $1; false when there is none.
fetch_header() {
  local out=$1 d
  d=$(enc_dir)
  rm -f "$out"
  if [ "$NEO_TARGET" = disk ]; then
    [ -f "$d/$HEADER" ] || return 1
    cp "$d/$HEADER" "$out"
  else
    printf -- '-get "%s/%s" "%s"\n' "$d" "$HEADER" "$out" | sftp_batch >/dev/null 2>&1 ||
      die "cannot reach $NEO_SSH_DEST over SFTP"
    [ -s "$out" ] || return 1
  fi
}

# Upload $1 as the header: written next to it, then renamed over it.
put_header() {
  local file=$1 d
  d=$(enc_dir)
  if [ "$NEO_TARGET" = disk ]; then
    cp "$file" "$d/$HEADER.tmp"
    sync "$d/$HEADER.tmp"
    mv -f "$d/$HEADER.tmp" "$d/$HEADER"
    return
  fi
  printf -- '-rm "%s/%s.tmp"\nput "%s" "%s/%s.tmp"\nrename "%s/%s.tmp" "%s/%s"\n' \
    "$d" "$HEADER" "$file" "$d" "$HEADER" "$d" "$HEADER" "$d" "$HEADER" | sftp_batch >/dev/null 2>&1 ||
    # Servers without posix-rename refuse to rename over a file.
    printf -- 'rm "%s/%s"\nrename "%s/%s.tmp" "%s/%s"\n' \
      "$d" "$HEADER" "$d" "$HEADER" "$d" "$HEADER" | sftp_batch >/dev/null ||
    die "cannot write $d/$HEADER at $NEO_SSH_DEST"
}

# The repository folders at the destination.
list_repos() {
  local d
  d=$(enc_dir)
  if [ "$NEO_TARGET" = disk ]; then
    find "$d" -mindepth 1 -maxdepth 1 -type d -name 'restic-*' -printf '%f\n'
  else
    printf 'ls -1 "%s"\n' "$d" | sftp_batch 2>/dev/null | sed -n 's|^.*/\{0,1\}\(restic-[^/]*\)/\{0,1\}$|\1|p'
  fi
}

remove_repo() {
  local d
  d=$(enc_dir)
  if [ "$NEO_TARGET" = disk ]; then
    rm -rf "${d:?}/$1"
  elif ! at_dest rm -rf "$d/$1" >/dev/null 2>&1; then
    log "could not remove $d/$1 at $NEO_SSH_DEST (no shell access?): delete it by hand, it holds the data of an old key"
  fi
}

restic_repo() {
  if [ "$NEO_TARGET" = disk ]; then
    echo "$(enc_dir)/$1"
  else
    echo "sftp:$NEO_SSH_DEST:$(enc_dir)/$1"
  fi
}

restic_cmd() {
  local opts=()
  [ "$NEO_TARGET" = ssh ] && opts=(-o "sftp.command=$run_dir/ssh $NEO_SSH_DEST -s sftp")
  RESTIC_PASSWORD_FILE="$run_dir/password" RESTIC_CACHE_DIR="${CACHE_DIRECTORY:-/var/cache/neo-backup}" \
    restic -r "$(restic_repo "$repo")" "${opts[@]}" "$@"
}

# Header recipients: this server and the admins.
recipients() {
  cat "$NEO_HOST_KEY.pub"
  [ -z "${NEO_RECIPIENTS:-}" ] || printf '%s\n' "$NEO_RECIPIENTS"
}

new_key() {
  repo="restic-$(date -u +%Y%m%d-%H%M%S)"
  head -c 32 /dev/urandom | base64 -w0 >"$run_dir/password"
}

write_header() {
  local out=$1
  recipients >"$run_dir/recipients"
  {
    echo "version=1"
    echo "format=restic"
    echo "repository=$repo"
    echo "password=$(cat "$run_dir/password")"
  } | age -a -R "$run_dir/recipients" -o "$out"
}

backup_encrypted() {
  local src old="" rc=0
  umask 077
  [ "$NEO_TARGET" = disk ] || ssh_wrapper
  ensure_enc_dir

  repo=""
  if fetch_header "$run_dir/header.age"; then
    age -d -i "$NEO_HOST_KEY" -o "$run_dir/header" "$run_dir/header.age" 2>/dev/null ||
      die "$(enc_dir)/$HEADER at the destination is not encrypted for this server's key: it belongs to another server or an old key. Pick another Backup folder, or delete that folder's content for a fresh backup."
    repo=$(sed -n 's/^repository=//p' "$run_dir/header")
    sed -n 's/^password=//p' "$run_dir/header" | tr -d '\n' >"$run_dir/password"
    rm -f "$run_dir/header"
    case "$repo" in restic-*) ;; *) die "$HEADER names no repository" ;; esac
  fi

  local fresh=0
  if [ -z "$repo" ]; then
    log "no $HEADER at the destination: new encryption key"
    new_key
    fresh=1
  elif [ "${NEO_ROTATE:-false}" = true ]; then
    old=$repo
    log "rotating the encryption key: new repository, full backup, then $old is removed"
    new_key
    fresh=1
  fi
  if [ "$fresh" = 1 ]; then
    restic_cmd init >>"$NEO_LOG" 2>&1 || {
      tail -n 20 "$NEO_LOG" >&2
      die "restic init of $repo failed"
    }
  fi
  restic_cmd unlock >/dev/null 2>&1 || true

  source_folder
  local args=(backup --host "$NEO_HOSTNAME" --tag neo --group-by "host,tags")
  while IFS= read -r e; do
    e=${e#/}
    e=${e%/}
    [ -n "$e" ] || continue
    # As rsync: a path is below the folder, a plain name matches anywhere.
    case "$e" in */*) args+=(--exclude "$src/$e") ;; *) args+=(--exclude "$e") ;; esac
  done <<<"${NEO_EXCLUDES:-}"

  log "backing up $NEO_SOURCE_DIR to $(restic_repo "$repo") (encrypted)"
  (cd "$src" && restic_cmd "${args[@]}" .) >"$NEO_LOG" 2>&1 || rc=$?
  # 3: some files could not be read; the snapshot exists without them.
  if [ "$rc" = 3 ]; then
    log "some files could not be read (see $NEO_LOG)"
  elif [ "$rc" != 0 ]; then
    tail -n 40 "$NEO_LOG" >&2
    die "restic backup failed, see $NEO_LOG"
  fi
  tail -n 12 "$NEO_LOG"

  # The header goes up after the first snapshot: a header always opens a backup.
  write_header "$run_dir/header.new"
  put_header "$run_dir/header.new"

  restic_cmd forget --tag neo --group-by "" --keep-last "$NEO_KEEP" --prune >>"$NEO_LOG" 2>&1 ||
    log "removing old backups failed (see $NEO_LOG); the new one is complete"

  # Old key (rotation) and folders of runs that never got a header.
  local r
  for r in $(list_repos); do
    if [ "$r" != "$repo" ]; then
      log "removing $r (no longer opened by $HEADER)"
      remove_repo "$r"
    fi
  done
  [ -z "$old" ] || log "key rotated: $old is gone, $repo uses the new key"
  log "done"
}

# Local backup snapshots, after every destination ran: kept are the newest one
# each ZFS destination received (the base of its next incremental send). While
# an enabled ZFS destination has never received one from this server (first
# run failed, or the machine was restored), older ones stay: one of them may be
# its base.
prune_snapshots() {
  pruned=1
  [ -n "$snap" ] || return 0
  local keep="" unknown=0 f n s
  for f in "$state_dir"/zfs-base-*; do
    [ -e "$f" ] || continue
    n=${f##*/zfs-base-}
    if grep -qx -- "$n" <<<"${NEO_ZFS_ALL:-}"; then
      keep+="$(cat "$f")"$'\n'
    else
      rm -f "$f"
    fi
  done
  while IFS= read -r n; do
    [ -z "$n" ] || [ -e "$state_dir/zfs-base-$n" ] || unknown=1
  done <<<"${NEO_ZFS_ACTIVE:-}"
  for s in $(zfs list -H -t snapshot -o name -d 1 "$NEO_DATASET" | sed -n "s|^.*@\($PREFIX.*\)\$|\1|p"); do
    if grep -qx -- "$s" <<<"$keep"; then
      continue
    elif [ "$s" = "$snap" ] || [ "$unknown" = 0 ]; then
      zfs destroy "$NEO_DATASET@$s" || true
    fi
  done
}

# One destination, in a subshell: variables, die and the ssh command stay there.
backup_to() {
  # shellcheck source=/dev/null
  . "$1"
  read -r -a ssh_cmd <<<"${NEO_SSH:-}"
  log "starting ($NEO_FORMAT to $NEO_TARGET)"
  if [ "$NEO_TARGET" = ssh ]; then
    # ssh exits 255 only for its own failures (connect, auth). Restricted
    # shells such as rsync.net's reject `true`, so the remote status is ignored.
    local rc=0 err
    err=$(at_dest true 2>&1 >/dev/null) || rc=$?
    [ "$rc" != 255 ] || die "cannot log in to $NEO_SSH_DEST: $err"
  fi
  case "$NEO_FORMAT" in
  files) backup_files ;;
  encrypted) backup_encrypted ;;
  zfs) backup_zfs ;;
  *) die "unknown format $NEO_FORMAT" ;;
  esac
}

# One backup at a time (the nightly run waits for a key rotation).
exec 9>/run/neo-backup.lock
flock 9

log "starting at $(date)"
mapfile -t destinations <<<"${NEO_DESTINATIONS:-}"
# One moment for every destination.
[ -z "$NEO_DATASET" ] || take_snapshot

failed=()
done_count=0
for entry in "${destinations[@]}"; do
  [ -n "$entry" ] || continue
  name=${entry%%=*}
  file=${entry#*=}
  # shellcheck source=/dev/null
  if [ "${NEO_ROTATE:-false}" = true ] && [ "$(. "$file" && echo "$NEO_FORMAT")" != encrypted ]; then
    continue
  fi
  set +e
  (
    set -e
    backup_to "$file"
  ) </dev/null
  rc=$?
  set -e
  if [ "$rc" = 0 ]; then
    done_count=$((done_count + 1))
  else
    log "backup to $name failed, going on with the next destination"
    failed+=("$name")
  fi
done
prune_snapshots

if [ "${#failed[@]}" -gt 0 ]; then
  die "$done_count destination(s) done, failed: ${failed[*]}"
fi
log "all $done_count destination(s) done"
