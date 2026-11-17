# neo-auto-update (default.nix): one scheduled `neo update` + `neo activate`,
# its run history, and the restart the new system needs.
#
# Env (set by default.nix):
#   NEO_HISTORY_DIR             run history (<run>.json, <run>.log, last.json)
#   NEO_ACTIVATION_MODE         switch | boot
#   NEO_REBOOT_WHEN_REQUIRED    true: restart when a switch needs one
#   NEO_REBOOT_AFTER_BOOT       true: restart after a boot activation
#   NEO_GC_OLDER_THAN           nix-collect-garbage age first, "" = no GC
#   NEO_CLI                     the neo CLI
#   NEO_RESTART_CHECK           neo-restart-check (restart-check.sh)

# Start of the run (the same in docker-updater/update.sh): run_id, history file and log,
# all further output also to the log. The web UI reads <run_id>.json/.log.
run_id=$(date -u +%Y%m%dT%H%M%SZ)-$$
started_at=$(date -Iseconds)
mkdir -p "$NEO_HISTORY_DIR"
hist=$NEO_HISTORY_DIR/$run_id.json
log_file=$NEO_HISTORY_DIR/$run_id.log
exec > >(tee -a "$log_file") 2>&1

before=$(readlink /nix/var/nix/profiles/system || echo none)
after=none
running=none
rc=""
changed=false
failed=false
reboot_pending=false
restart_reasons='[]'
restart_not_applied=false
reboot_scheduled=false
reboot_scheduled_at=""
finished_at=""
activation_mode=$NEO_ACTIVATION_MODE
reboot_when_required=$NEO_REBOOT_WHEN_REQUIRED
reboot_after_boot=$NEO_REBOOT_AFTER_BOOT
# One automatic restart per system and reasons (restart_attempted).
attempt_file=$NEO_HISTORY_DIR/restart-attempt.json
hist_done=0

write_hist() {
  local in_progress=$1
  jq -n \
    --arg run_id "$run_id" \
    --arg started "$started_at" \
    --arg before "$before" \
    --arg after "$after" \
    --arg running "$running" \
    --arg activation_mode "$activation_mode" \
    --arg reboot_scheduled_at "$reboot_scheduled_at" \
    --argjson in_progress "$in_progress" \
    --argjson changed "$changed" \
    --argjson failed "$failed" \
    --argjson reboot_pending "$reboot_pending" \
    --argjson restart_reasons "$restart_reasons" \
    --argjson restart_not_applied "$restart_not_applied" \
    --argjson reboot_scheduled "$reboot_scheduled" \
    --arg rc "$rc" \
    --arg finished "$finished_at" \
    --arg log "$run_id.log" \
    '{kind:"system",run_id:$run_id,started_at:$started,in_progress:$in_progress,generation_before:$before,generation_after:$after,running_system:$running,activation_mode:$activation_mode,changed:$changed,failed:$failed,reboot_pending:$reboot_pending,restart_reasons:$restart_reasons,restart_not_applied:$restart_not_applied,reboot_scheduled:$reboot_scheduled,reboot_scheduled_at:(if $reboot_scheduled_at == "" then null else $reboot_scheduled_at end),exit_code:(if $rc == "" then null else ($rc|tonumber) end),finished_at:(if $finished == "" then null else $finished end),log:$log}' \
    > "$hist"
}

# reboot_pending: the system profile needs a restart to take full effect
# (nix/modules/core/restart.nix: boot default ahead, kernel, Docker on old
# settings, raised restart triggers). Store paths on both sides: the
# profile link alone reads system-N-link and never equals a store path.
check_restart() {
  local script=/nix/var/nix/profiles/system/sw/bin/neo-restart-required status
  if [ -x "$script" ] && status=$("$script" --json); then
    reboot_pending=true
  elif [ -x "$script" ]; then
    reboot_pending=false
  else
    # A system built before the restart check: only the boot default counts.
    status='{"reasons":[]}'
    if [ "$(readlink -f /nix/var/nix/profiles/system)" != "$(readlink -f /run/current-system)" ]; then
      reboot_pending=true
      status='{"reasons":[{"id":"system","text":"The new version is the boot default and runs after a restart."}]}'
    else
      reboot_pending=false
    fi
  fi
  restart_reasons=$(jq -c '.reasons // []' <<<"$status" 2>/dev/null || echo '[]')
}

finalize() {
  [ "$hist_done" -eq 1 ] && return
  hist_done=1
  trap - EXIT INT TERM HUP
  after=$(readlink /nix/var/nix/profiles/system || echo none)
  running=$(readlink -f /run/current-system || echo none)
  if [ "$before" != "$after" ]; then changed=true; fi
  check_restart
  if [ "${1:-}" = abort ]; then
    failed=true
    if [ -z "$rc" ]; then rc=1; fi
  elif [ -n "$rc" ] && [ "$rc" -ne 0 ]; then
    failed=true
  fi
  finished_at=$(date -Iseconds)
  write_hist false
  echo "Wrote $hist (changed=$changed failed=$failed reboot_pending=$reboot_pending reasons=$restart_reasons reboot_scheduled=$reboot_scheduled mode=$activation_mode exit=$rc); last.json -> $run_id.json"
}

# After a successful update whose system needs a restart, schedule a delayed
# host cycle. The delay lets this oneshot exit (history + OnSuccess
# supervise) first. Never after a failed run, never twice for the same
# system and reasons: if a restart did not clear them, it would not the
# second time either (restart_not_applied, Hermes reports it).
maybe_schedule_reboot() {
  if [ "$failed" = true ]; then
    return 0
  fi
  if [ "$reboot_pending" != true ]; then
    echo "No restart needed"
    return 0
  fi
  echo "Restart needed: $(jq -r 'map(.text) | join(" ")' <<<"$restart_reasons")"
  if [ "$activation_mode" = boot ] && [ "$reboot_after_boot" != true ]; then
    echo "rebootAfterBoot=false; leaving it for a manual restart"
    return 0
  fi
  if [ "$activation_mode" != boot ] && [ "$reboot_when_required" != true ]; then
    echo "rebootWhenRequired=false; leaving it for a manual restart"
    return 0
  fi
  local key boot_id
  key="$(readlink -f /nix/var/nix/profiles/system) $(jq -r 'map(.id) | sort | join(",")' <<<"$restart_reasons")"
  boot_id=$(cat /proc/sys/kernel/random/boot_id)
  if [ -f "$attempt_file" ] \
    && [ "$(jq -r .key "$attempt_file" 2>/dev/null)" = "$key" ] \
    && [ "$(jq -r .boot_id "$attempt_file" 2>/dev/null)" != "$boot_id" ]; then
    echo "WARNING: already restarted once for this version and the same reasons, and they are still there; not restarting again" >&2
    restart_not_applied=true
    write_hist false
    return 0
  fi
  jq -n --arg key "$key" --arg boot_id "$boot_id" --arg at "$(date -Iseconds)" \
    '{key: $key, boot_id: $boot_id, at: $at}' > "$attempt_file"
  schedule_reboot "mode=$activation_mode"
}

schedule_reboot() {
  # /run/neo/restart-dry-run (tests): decide, but do not restart.
  if [ -e /run/neo/restart-dry-run ]; then
    echo "Dry run: would schedule a host cycle in 60s ($1)"
    return 0
  fi
  echo "Scheduling delayed host cycle ($1, unit=neo-auto-update-apply, on-active=60s)"
  # Clear a leftover transient unit from a prior run (best-effort).
  /run/wrappers/bin/sudo systemctl reset-failed neo-auto-update-apply.service 2>/dev/null || true
  /run/wrappers/bin/sudo systemctl stop neo-auto-update-apply.timer 2>/dev/null || true
  /run/wrappers/bin/sudo systemctl stop neo-auto-update-apply.service 2>/dev/null || true
  if /run/wrappers/bin/sudo systemd-run \
    --on-active=60s \
    --unit=neo-auto-update-apply \
    --description="neo-auto-update delayed restart for the update" \
    /run/current-system/sw/bin/systemctl reboot; then
    reboot_scheduled=true
    reboot_scheduled_at=$(date -Iseconds)
    write_hist false
    echo "Delayed host cycle scheduled at $reboot_scheduled_at (reboot_scheduled=true); host should reboot in ~60s"
  else
    echo "ERROR: failed to schedule delayed host cycle via systemd-run" >&2
  fi
}

write_hist true
ln -sfn "$run_id.json" "$NEO_HISTORY_DIR/last.json"
echo "Started run $run_id; last.json -> $run_id.json (in_progress) activation_mode=$activation_mode reboot_after_boot=$reboot_after_boot"
trap 'finalize abort' EXIT INT TERM HUP
if [ -n "$NEO_GC_OLDER_THAN" ]; then
  /run/wrappers/bin/sudo nix-collect-garbage --delete-older-than "$NEO_GC_OLDER_THAN" || true
fi
set +e
"$NEO_CLI" --profile server update && "$NEO_CLI" --profile server activate --mode "$activation_mode"
rc=$?
set -e
finalize ok
maybe_schedule_reboot
# Check the server once it runs again after the restart this run
# needs, whenever that happens (neo-auto-update-verify). With a
# scheduled restart, Hermes supervises the run then instead of now.
if [ "$failed" != true ] && [ "$reboot_pending" = true ]; then
  "$NEO_RESTART_CHECK" record "$run_id" "$restart_reasons" "$activation_mode" "$reboot_scheduled" \
    || echo "WARNING: could not record the check after the restart" >&2
elif [ "$failed" != true ]; then
  "$NEO_RESTART_CHECK" forget || true
fi
exit "$rc"
