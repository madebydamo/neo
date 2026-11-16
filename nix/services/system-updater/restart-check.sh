# neo-restart-check record|forget|verify: check the server after a restart
# that an update needed (nix/services/system-updater/default.nix).
#
#   record RUN_ID REASONS_JSON MODE SCHEDULED
#       The updater, after a successful run whose system needs a restart
#       (scheduled or not): remember the run, the system it installed, why and
#       this boot's id in restart-check.json. The check then runs in the next
#       boot, whatever restarts the server (the updater, Settings → Power, a
#       power cut).
#   forget
#       The updater, after a successful run that needs no restart: drop a check
#       recorded in this boot (its reasons are gone without a restart).
#   verify
#       neo-auto-update-verify, a few minutes after each boot. When a check is
#       due (recorded in another boot), wait until the boot has settled and
#       record in that run's history JSON (`restart_check`) whether the server
#       came back on the system the update installed, which restart reasons are
#       left and which units failed. restart-checked.json then points at the
#       run for Hermes (neo-hermes-supervise restart).
#
# The NEO_* variables point it at other paths (flake check `restart-check`).
set -euo pipefail

dir=${NEO_UPDATER_DIR:?NEO_UPDATER_DIR is not set}
marker=$dir/restart-check.json
boot_id=$(cat "${NEO_BOOT_ID_FILE:-/proc/sys/kernel/random/boot_id}")
profile=${NEO_SYSTEM_PROFILE:-/nix/var/nix/profiles/system}
booted_link=${NEO_RESTART_BOOTED:-/run/booted-system}

marker_boot() {
  jq -r '.boot_id // ""' "$marker" 2>/dev/null || true
}

case "${1:-}" in
record)
  [ "$#" -eq 5 ] || {
    echo "usage: neo-restart-check record RUN_ID REASONS_JSON MODE SCHEDULED" >&2
    exit 2
  }
  jq -n \
    --arg run_id "$2" \
    --argjson reasons "$3" \
    --arg mode "$4" \
    --argjson scheduled "$5" \
    --arg system "$(readlink -f "$profile")" \
    --arg boot_id "$boot_id" \
    --arg at "$(date -Iseconds)" \
    '{run_id: $run_id, system: $system, reasons: $reasons, activation_mode: $mode,
      reboot_scheduled: $scheduled, boot_id: $boot_id, at: $at}' >"$marker.tmp"
  mv "$marker.tmp" "$marker"
  echo "Restart check recorded for run $2: runs in the next boot"
  ;;

forget)
  if [ -f "$marker" ] && [ "$(marker_boot)" = "$boot_id" ]; then
    rm -f "$marker"
    echo "Restart check of this boot dropped: no restart needed any more"
  fi
  ;;

verify)
  if [ ! -f "$marker" ]; then
    echo "No restart to check"
    exit 0
  fi
  run_id=$(jq -r '.run_id // ""' "$marker" 2>/dev/null || true)
  if [ -z "$run_id" ]; then
    echo "Unreadable $marker; dropping it" >&2
    rm -f "$marker"
    exit 1
  fi
  if [ "$(marker_boot)" = "$boot_id" ]; then
    echo "Run $run_id needs a restart that has not happened yet"
    exit 0
  fi

  # Let the boot finish (or give up on it) before judging it.
  if [ -z "${NEO_RESTART_CHECK_NO_WAIT:-}" ]; then
    echo "Waiting for the boot to settle"
    timeout "${NEO_RESTART_CHECK_SETTLE:-900}" systemctl is-system-running --wait >/dev/null 2>&1 || true
  fi
  if [ -n "${NEO_SYSTEM_STATE+set}" ]; then
    state=$NEO_SYSTEM_STATE
  else
    state=$(systemctl is-system-running 2>/dev/null || true)
  fi
  if [ -n "${NEO_FAILED_UNITS+set}" ]; then
    failed_units=$NEO_FAILED_UNITS
  else
    failed_units=$(systemctl list-units --state=failed --plain --no-legend 2>/dev/null | awk '{print $1}' || true)
  fi

  required=${NEO_RESTART_REQUIRED:-$profile/sw/bin/neo-restart-required}
  left='[]'
  if [ -x "$required" ]; then
    left=$("$required" --json 2>/dev/null | jq -c '.reasons // []' 2>/dev/null || true)
    [ -n "$left" ] || left='[]'
  fi

  check=$(jq -c \
    --arg at "$(date -Iseconds)" \
    --arg boot_id "$boot_id" \
    --arg booted "$(readlink -f "$booted_link")" \
    --arg profile "$(readlink -f "$profile")" \
    --arg state "$state" \
    --arg failed "$failed_units" \
    --argjson left "$left" \
    '{checked_at: $at, boot_id: $boot_id,
      expected_system: .system, booted_system: $booted, profile_system: $profile,
      booted_expected: ($booted == .system),
      reasons_before: .reasons, reasons_left: $left,
      system_state: $state,
      failed_units: ($failed | split("\n") | map(select(. != "")))}
     | .ok = (.booted_expected and (.reasons_left | length) == 0 and (.failed_units | length) == 0)' \
    "$marker")

  hist=$dir/$run_id.json
  if [ -f "$hist" ]; then
    jq --argjson c "$check" '. + {restart_check: $c}' "$hist" >"$hist.tmp"
  else
    jq -n --arg run_id "$run_id" --argjson c "$check" '{kind: "system", run_id: $run_id, restart_check: $c}' >"$hist.tmp"
  fi
  mv "$hist.tmp" "$hist"
  ln -sfn "$run_id.json" "$dir/restart-checked.json"
  rm -f "$marker"

  jq -r '"Restart check for run \(input_filename | split("/") | last | rtrimstr(".json")): ok=\(.restart_check.ok) booted_expected=\(.restart_check.booted_expected) state=\(.restart_check.system_state) reasons_left=\(.restart_check.reasons_left | map(.id) | join(",")) failed_units=\(.restart_check.failed_units | join(","))"' "$hist"
  ;;

*)
  echo "usage: neo-restart-check record|forget|verify" >&2
  exit 2
  ;;
esac
