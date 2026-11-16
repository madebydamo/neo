# neo-restart-check (nix/services/system-updater/restart-check.sh) against fake
# systems and boots: a recorded check runs once, in the next boot only, and
# records in the run's history whether the server came back as expected.
{...}: {
  perSystem = {pkgs, ...}: {
    checks.restart-check =
      pkgs.runCommand "restart-check" {
        nativeBuildInputs = [pkgs.jq pkgs.coreutils pkgs.gawk];
      } ''
        set -euo pipefail
        script=${../services/system-updater/restart-check.sh}
        fails=0
        fail() { echo "FAIL $*" >&2; fails=1; }
        ok() { echo "ok   $*"; }

        mkdir -p hist sysA sysB
        echo boot-1 > boot_id
        ln -s sysA profile
        ln -s sysA booted
        # Fake neo-restart-required: prints LEFT as its reasons.
        cat > required <<'EOS'
        #!/bin/sh
        echo "{\"required\": false, \"reasons\": ''${LEFT:-[]}}"
        EOS
        chmod +x required
        export NEO_UPDATER_DIR=$PWD/hist NEO_BOOT_ID_FILE=$PWD/boot_id NEO_SYSTEM_PROFILE=$PWD/profile \
          NEO_RESTART_BOOTED=$PWD/booted NEO_RESTART_REQUIRED=$PWD/required NEO_RESTART_CHECK_NO_WAIT=1 \
          NEO_SYSTEM_STATE=running NEO_FAILED_UNITS=""
        rc() { bash "$script" "$@" >/dev/null; }
        kernel='[{"id":"kernel","text":"New Linux kernel."}]'

        # Recorded in this boot: verify waits for the next one.
        echo '{"kind":"system","run_id":"r1","changed":true}' > hist/r1.json
        rc record r1 "$kernel" switch true
        [ "$(jq -r '.system, .boot_id, .reboot_scheduled' hist/restart-check.json | paste -sd' ')" = "$PWD/sysA boot-1 true" ] \
          && ok "record" || fail "record: $(cat hist/restart-check.json)"
        rc verify
        [ -f hist/restart-check.json ] && ! jq -e .restart_check hist/r1.json >/dev/null \
          && ok "same boot: not checked yet" || fail "same boot was checked"

        # forget drops a check of this boot only.
        rc forget
        [ ! -f hist/restart-check.json ] && ok "forget in the same boot" || fail "forget kept the check"
        rc record r1 "$kernel" switch true
        echo boot-2 > boot_id
        rc forget
        [ -f hist/restart-check.json ] && ok "forget keeps a check of another boot" || fail "forget dropped another boot's check"

        # Next boot, back on the new system, nothing left: ok, merged into the run, once.
        rc verify
        [ "$(jq -r '.changed, .restart_check.ok, .restart_check.booted_expected, .restart_check.reasons_before[0].id' hist/r1.json | paste -sd' ')" = "true true true kernel" ] \
          && ok "came back as expected" || fail "expected ok: $(cat hist/r1.json)"
        [ "$(readlink hist/restart-checked.json)" = r1.json ] && [ ! -f hist/restart-check.json ] \
          && ok "restart-checked.json, check consumed" || fail "symlink or marker wrong"
        cp hist/r1.json before.json
        rc verify
        cmp -s before.json hist/r1.json && ok "checks once" || fail "checked twice"

        # Came back on another system (boot mode, older generation picked).
        echo boot-2 > boot_id
        echo '{"kind":"system","run_id":"r2","changed":true}' > hist/r2.json
        ln -sfn sysB profile
        rc record r2 '[{"id":"system","text":"Boot default ahead."}]' boot true
        echo boot-3 > boot_id
        LEFT='[{"id":"system","text":"Boot default ahead."}]' rc verify
        [ "$(jq -r '.restart_check | .ok, .booted_expected, .reasons_left[0].id' hist/r2.json | paste -sd' ')" = "false false system" ] \
          && ok "booted another system" || fail "other system: $(cat hist/r2.json)"

        # Failed units, history cleared in between.
        ln -sfn sysA profile
        rc record r3 "$kernel" switch false
        echo boot-4 > boot_id
        NEO_FAILED_UNITS=$'docker-foo.service\nbar.service' NEO_SYSTEM_STATE=degraded rc verify
        [ "$(jq -r '.run_id, .restart_check.ok, (.restart_check.failed_units | join(",")), .restart_check.system_state' hist/r3.json | paste -sd' ')" = "r3 false docker-foo.service,bar.service degraded" ] \
          && ok "failed units, no history" || fail "failed units: $(cat hist/r3.json)"

        [ "$fails" = 0 ]
        touch $out
      '';
  };
}
