# Backup store: the storage and one home folder per client
# (docs/design/backup-restore.md §6). Runs as root before the store's sshd.
#
# Env (set by default.nix):
#   NEO_STORAGE    dataset | folder
#   NEO_PATH       where the storage is mounted
#   NEO_DATASET    ZFS dataset (dataset)
#   NEO_ENCRYPTED  true | false (dataset)
#   NEO_KEY_FILE   passphrase file (encrypted)
#   NEO_CLIENTS    lines "<client> <files|encrypted|zfs>"
#
# Sent ZFS backups land in <dataset>/<client>/<name> and inherit its encryption.

die() {
  echo "backup-store: $*" >&2
  exit 1
}

mount_dataset() {
  [ "$(zfs get -H -o value mounted "$1")" = yes ] || zfs mount "$1"
}

setup_key() {
  local status
  status=$(zfs get -H -o value keystatus "$NEO_DATASET")
  if [ "$status" = available ]; then
    # Also applies a changed encryption key: the old one is still loaded here.
    [ "$NEO_ENCRYPTED" != true ] ||
      zfs change-key -o keyformat=passphrase -o "keylocation=file://$NEO_KEY_FILE" "$NEO_DATASET"
  else
    [ "$NEO_ENCRYPTED" = true ] || die "$NEO_DATASET is encrypted: turn on Encrypt the stored backups and set its key"
    zfs load-key -L "file://$NEO_KEY_FILE" "$NEO_DATASET" || die "the encryption key does not open $NEO_DATASET"
  fi
}

setup_dataset() {
  local pool=${NEO_DATASET%%/*}
  zpool list -H "$pool" >/dev/null 2>&1 || die "pool $pool not found: is its disk attached?"
  if [ "$NEO_ENCRYPTED" = true ] && [ "$(wc -c <"$NEO_KEY_FILE")" -lt 8 ]; then
    die "the encryption key needs at least 8 characters"
  fi

  if zfs list -H -o name "$NEO_DATASET" >/dev/null 2>&1; then
    local enc
    enc=$(zfs get -H -o value encryption "$NEO_DATASET")
    if [ "$enc" = off ]; then
      [ "$NEO_ENCRYPTED" != true ] || die "$NEO_DATASET exists without encryption, and ZFS cannot encrypt a dataset in place: pick another dataset, or move the backups and destroy it"
    else
      setup_key
    fi
    [ "$(zfs get -H -o value mountpoint "$NEO_DATASET")" = "$NEO_PATH" ] || zfs set "mountpoint=$NEO_PATH" "$NEO_DATASET"
  else
    local opts=(-o "mountpoint=$NEO_PATH" -o com.sun:auto-snapshot=false)
    if [ "$NEO_ENCRYPTED" = true ]; then
      opts+=(-o encryption=aes-256-gcm -o keyformat=passphrase -o "keylocation=file://$NEO_KEY_FILE")
    fi
    zfs create -p "${opts[@]}" "$NEO_DATASET"
  fi
  mount_dataset "$NEO_DATASET"
}

setup_client() {
  local client=$1 user="backup-$1" home="$NEO_PATH/$1"
  if [ "$NEO_STORAGE" = dataset ]; then
    local ds="$NEO_DATASET/$client"
    zfs list -H -o name "$ds" >/dev/null 2>&1 || zfs create "$ds"
    mount_dataset "$ds"
    zfs allow -u "$user" create,receive,destroy,hold,mount,rollback,send,canmount,userprop "$ds"
  else
    mkdir -p "$home"
  fi
  # A rebuilt store can give the account another uid.
  if [ "$(stat -c %U "$home")" != "$user" ]; then
    chown -R "$user:backup-store" "$home"
  fi
  chmod 0700 "$home"
}

if [ "$NEO_STORAGE" = dataset ]; then
  setup_dataset
else
  mkdir -p "$NEO_PATH"
fi
chown root:root "$NEO_PATH"
# Readable, not 0711: rsync opens every parent folder of its destination.
chmod 0755 "$NEO_PATH"

while read -r client _; do
  [ -z "$client" ] || setup_client "$client"
done <<<"$NEO_CLIENTS"
