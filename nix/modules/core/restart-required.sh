# neo-restart-required [--json] [SYSTEM]: does SYSTEM need a restart of the
# server to take full effect? Exit 0 = yes (reasons printed), 1 = no.
# See nix/modules/core/restart.nix. The NEO_RESTART_* variables point the
# checks at other paths (flake check `restart-required`).
set -uo pipefail

json=false
if [ "${1:-}" = --json ]; then
  json=true
  shift
fi
system=$(readlink -f "${1:-/nix/var/nix/profiles/system}")
running=$(readlink -f "${NEO_RESTART_RUNNING:-/run/current-system}")
booted=$(readlink -f "${NEO_RESTART_BOOTED:-/run/booted-system}")
baseline=${NEO_RESTART_BASELINE:-/run/neo/restart-baseline.json}

ids=()
texts=()
add() {
  ids+=("$1")
  texts+=("$2")
}

# Kernel version of a system (its modules directory name), "" when unknown.
kernel_version() {
  local d
  for d in "$1"/kernel-modules/lib/modules/*; do
    [ -d "$d" ] && { basename "$d"; return; }
  done
}

# 1. The boot default is not what runs (activationMode=boot).
if [ "$system" != "$running" ]; then
  add system "The new version is the boot default and runs after a restart."
fi

# 2. Kernel and boot files.
if [ -e "$booted/kernel" ] && [ -e "$system/kernel" ]; then
  changed=false
  for f in kernel initrd kernel-modules; do
    [ "$(readlink -f "$booted/$f")" = "$(readlink -f "$system/$f")" ] || changed=true
  done
  if [ "$changed" = true ]; then
    old=$(kernel_version "$booted")
    new=$(kernel_version "$system")
    if [ -n "$old" ] && [ "$old" != "$new" ]; then
      add kernel "New Linux kernel ($old → $new)."
    else
      add kernel "New boot files for the Linux kernel ${new:-}."
    fi
  elif ! cmp -s "$booted/kernel-params" "$system/kernel-params"; then
    add kernel "Changed kernel boot options."
  fi
fi

# 3. Docker runs with the settings of an older version.
want=$(grep -hoE -- '--config-file=[^ ]+' \
  "$system"/etc/systemd/system/docker.service \
  "$system"/etc/systemd/system/docker.service.d/*.conf 2>/dev/null | tail -n 1)
if [ -n "$want" ]; then
  if [ -n "${NEO_RESTART_DOCKERD_CMDLINE+set}" ]; then
    cmdline=$NEO_RESTART_DOCKERD_CMDLINE
  else
    cmdline=""
    pid=$(systemctl show -p MainPID --value docker.service 2>/dev/null || echo 0)
    if [ "${pid:-0}" -gt 0 ] 2>/dev/null; then
      cmdline=$(tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null || true)
    fi
  fi
  if [ -n "$cmdline" ]; then
    have=$(grep -oE -- '--config-file=[^ ]+' <<<"$cmdline" | tail -n 1)
    if [ "$have" != "$want" ]; then
      add docker "Docker still runs with the settings of an older version."
    fi
  fi
fi

# 4. Restart triggers raised above the level they started with in this boot.
if [ -f "$system/neo-restart.json" ] && [ -f "$baseline" ]; then
  while IFS=$'\t' read -r id reason; do
    [ -n "$id" ] && add "trigger:$id" "$reason"
  done < <(jq -r --slurpfile base "$baseline" '
      .triggers // {} | to_entries[]
      | select(($base[0][.key] // null) != null and .value.level > $base[0][.key])
      | "\(.key)\t\(.value.reason)"
    ' "$system/neo-restart.json" 2>/dev/null)
fi

required=false
[ "${#ids[@]}" -gt 0 ] && required=true

if [ "$json" = true ]; then
  for i in "${!ids[@]}"; do
    jq -nc --arg id "${ids[$i]}" --arg text "${texts[$i]}" '{id: $id, text: $text}'
  done | jq -sc --argjson required "$required" --arg system "$system" \
    '{required: $required, system: $system, reasons: .}'
else
  for i in "${!ids[@]}"; do
    echo "${texts[$i]} (${ids[$i]})"
  done
fi

[ "$required" = true ]
