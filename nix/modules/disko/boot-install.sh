# Runs after GRUB's own install (boot.loader.grub.extraInstallCommands), on
# every switch and during nixos-install. Never fails the switch: a problem here
# is logged and the generation is still activated.
#
# NixOS installs GRUB for UEFI only, in the removable path
# (\EFI\BOOT\BOOTX64.EFI). Two gaps remain on real hardware:
#
# 1. BIOS (legacy) firmware: GRUB also goes into the disk's MBR + the 1M BIOS
#    boot partition. Only when that partition exists, so disks partitioned
#    before Neo added it keep booting UEFI-only and the switch does not fail.
# 2. UEFI firmware that never tries the removable path (seen at some hosters: PXE,
#    then stale entries of the previous OS, then the EFI Shell): a "NixOS"
#    firmware boot entry, created first in the boot order when missing, plus
#    a startup.nsh so the EFI Shell boots the disk as well.
#
# Env (set by default.nix): NEO_ESP, NEO_BIOS, NEO_BOOT_ENTRY, NEO_GRUB,
# NEO_EFI_LOADER, NEO_STARTUP_NSH.

log() { echo "neo-boot: $*" >&2; }

part=$(findmnt -nvo SOURCE --target "$NEO_ESP" 2>/dev/null) || part=""
part=$(readlink -f "$part" 2>/dev/null || true)
pname=$(basename "$part")
if [ -z "$part" ] || [ ! -e "/sys/class/block/$pname/partition" ]; then
  log "$NEO_ESP is not on a disk partition ($part), skipping"
  exit 0
fi
partnum=$(cat "/sys/class/block/$pname/partition")
dname=$(basename "$(readlink -f "/sys/class/block/$pname/..")")
disk=/dev/$dname

# BIOS boot partition (GPT type EF02) on the ESP's disk?
bios_part=""
for p in "/sys/class/block/$dname/$dname"*; do
  [ -e "$p/partition" ] || continue
  t=$(blkid -p -s PART_ENTRY_TYPE -o value "/dev/$(basename "$p")" 2>/dev/null || true)
  if [ "$t" = "21686148-6449-6e6f-744e-656564454649" ]; then
    bios_part=/dev/$(basename "$p")
    break
  fi
done

if [ "$NEO_BIOS" = 1 ]; then
  if [ -n "$bios_part" ]; then
    stamp="$NEO_ESP/grub/neo-bios-install"
    want="$NEO_GRUB $disk"
    if [ "$(cat "$stamp" 2>/dev/null)" != "$want" ]; then
      log "installing GRUB for BIOS boot on $disk"
      if "$NEO_GRUB/bin/grub-install" --target=i386-pc --boot-directory="$NEO_ESP" "$disk"; then
        echo "$want" >"$stamp"
      else
        log "WARNING: GRUB BIOS install on $disk failed; this disk boots via UEFI only"
      fi
    fi
  else
    log "no BIOS boot partition on $disk: UEFI boot only"
  fi
fi

if [ -n "$NEO_STARTUP_NSH" ] && ! cmp -s "$NEO_STARTUP_NSH" "$NEO_ESP/startup.nsh"; then
  if ! { cp "$NEO_STARTUP_NSH" "$NEO_ESP/startup.nsh.tmp" && mv "$NEO_ESP/startup.nsh.tmp" "$NEO_ESP/startup.nsh"; }; then
    log "WARNING: could not write $NEO_ESP/startup.nsh"
  fi
fi

[ "$NEO_BOOT_ENTRY" = 1 ] || exit 0
if [ ! -d /sys/firmware/efi/efivars ]; then
  exit 0 # booted via BIOS: no firmware boot entries
fi

partuuid=$(blkid -p -s PART_ENTRY_UUID -o value "$part" 2>/dev/null | tr '[:upper:]' '[:lower:]')
if [ -z "$partuuid" ]; then
  log "WARNING: no partition UUID for $part, firmware boot entry not checked"
  exit 0
fi
entries=$(efibootmgr -v 2>/dev/null) || {
  log "WARNING: cannot read the firmware boot entries, none added"
  exit 0
}
loader_lc=$(printf '%s' "$NEO_EFI_LOADER" | tr '[:upper:]' '[:lower:]')

# "NixOS" entries: keep the first that points at this ESP, drop duplicates and
# ones whose partition no longer exists (a reinstall with new partition UUIDs).
ours=""
while IFS= read -r line; do
  [ -n "$line" ] || continue
  num=$(printf '%s' "$line" | cut -c5-8)
  lc=$(printf '%s' "$line" | tr '[:upper:]' '[:lower:]')
  case "$lc" in
  *"$partuuid"*"$loader_lc"*)
    if [ -z "$ours" ]; then ours=$num; else efibootmgr -q -b "$num" -B || true; fi
    ;;
  *)
    other=$(printf '%s' "$lc" | sed -n 's/.*gpt,\([0-9a-f-]\{36\}\).*/\1/p')
    if [ -n "$other" ] && [ -d /dev/disk/by-partuuid ] && [ ! -e "/dev/disk/by-partuuid/$other" ]; then
      log "removing firmware boot entry Boot$num (its partition is gone)"
      efibootmgr -q -b "$num" -B || true
    fi
    ;;
  esac
done <<EOF
$(printf '%s\n' "$entries" | grep -E '^Boot[0-9A-Fa-f]{4}\*? NixOS([[:space:]]|$)')
EOF

# An existing entry keeps its place: the operator may have put network boot
# first on purpose (e.g. before a provider reinstall).
[ -z "$ours" ] || exit 0

log "adding firmware boot entry NixOS ($disk partition $partnum) first in the boot order"
if ! efibootmgr -q -c -d "$disk" -p "$partnum" -L NixOS -l "$NEO_EFI_LOADER"; then
  log "WARNING: could not add the firmware boot entry"
  exit 0
fi
new=$(efibootmgr | grep -E '^Boot[0-9A-Fa-f]{4}\*? NixOS([[:space:]]|$)' | head -1 | cut -c5-8)
order=$(efibootmgr | sed -n 's/^BootOrder: //p')
rest=$(printf '%s' "$order" | tr ',' '\n' | grep -v "^$new\$" | paste -sd, - || true)
efibootmgr -q -o "$new${rest:+,$rest}" || log "WARNING: could not move Boot$new first"
exit 0
