# neo-restart-required (nix/modules/core/restart.nix) against fake systems:
# each rule restarts exactly when it should, and nothing else does.
{...}: {
  perSystem = {pkgs, ...}: {
    checks.restart-required =
      pkgs.runCommand "restart-required" {
        nativeBuildInputs = [pkgs.jq pkgs.diffutils pkgs.gnugrep pkgs.coreutils];
      } ''
        set -euo pipefail
        script=${../modules/core/restart-required.sh}

        # mksys DIR KERNEL PARAMS CONFIGFILE TRIGGERS_JSON
        mksys() {
          local d=$1 k=$PWD/store/linux-$2
          # Like the store: systems with the same kernel link the same paths.
          mkdir -p "$k/modules/lib/modules/$2" "$d/etc/systemd/system/docker.service.d"
          echo "$2" > "$k/bzImage"; echo "$2" > "$k/initrd"
          ln -s "$k/bzImage" "$d/kernel"; ln -s "$k/initrd" "$d/initrd"; ln -s "$k/modules" "$d/kernel-modules"
          echo -n "$3" > "$d/kernel-params"
          [ -n "$4" ] && echo "ExecStart=/bin/dockerd --config-file=$4" > "$d/etc/systemd/system/docker.service.d/overrides.conf"
          echo "{\"triggers\": $5}" > "$d/neo-restart.json"
        }

        fails=0
        # expect NAME EXIT IDS SYSTEM BOOTED BASELINE_JSON DOCKERD_CMDLINE [RUNNING]
        expect() {
          local name=$1 want_rc=$2 want_ids=$3 sys=$4 boot=$5 run=''${8:-$4}
          echo "$6" > base.json
          set +e
          res=$(NEO_RESTART_RUNNING=$run NEO_RESTART_BOOTED=$boot NEO_RESTART_BASELINE=base.json \
            NEO_RESTART_DOCKERD_CMDLINE="$7" bash "$script" --json "$sys")
          rc=$?
          set -e
          ids=$(jq -r '[.reasons[].id] | join(",")' <<<"$res")
          if [ "$rc" != "$want_rc" ] || [ "$ids" != "$want_ids" ]; then
            echo "FAIL $name: exit $rc (want $want_rc), reasons '$ids' (want '$want_ids'): $res" >&2
            fails=1
          else
            echo "ok   $name"
          fi
        }

        t1='{"core": {"level": 1, "reason": "Core.", "onEnable": false}}'
        t2='{"core": {"level": 2, "reason": "Core again.", "onEnable": false}}'
        tsvc='{"core": {"level": 1, "reason": "Core.", "onEnable": false}, "svc": {"level": 3, "reason": "Svc.", "onEnable": false}}'
        d=/dockerd
        mksys a 6.18.63 "quiet" /cfgA "$t1"
        mksys b 6.18.70 "quiet" /cfgA "$t1"
        mksys c 6.18.63 "quiet loglevel=4" /cfgA "$t1"
        mksys e 6.18.63 "quiet" /cfgB "$t1"
        mksys f 6.18.63 "quiet" /cfgA "$t2"
        mksys g 6.18.63 "quiet" /cfgA "$tsvc"
        mksys h 6.18.63 "quiet" "" "$t1"
        # same kernel version, other initrd
        mksys i 6.18.63 "quiet" /cfgA "$t1"; echo other > other-initrd; ln -sfn "$PWD/other-initrd" i/initrd

        expect "nothing changed"                1 ""           a a '{"core":1}' "$d --config-file=/cfgA"
        expect "boot default ahead (boot mode)" 0 "system,kernel" b a '{"core":1}' "$d --config-file=/cfgA" a
        expect "new kernel after a switch"      0 "kernel"     b a '{"core":1}' "$d --config-file=/cfgA"
        expect "new initrd, same kernel"        0 "kernel"     i a '{"core":1}' "$d --config-file=/cfgA"
        expect "kernel boot options"            0 "kernel"     c a '{"core":1}' "$d --config-file=/cfgA"
        expect "dockerd on old settings"        0 "docker"     e a '{"core":1}' "$d --config-file=/cfgA"
        expect "dockerd restarted correctly"    1 ""           e a '{"core":1}' "$d --config-file=/cfgB"
        expect "docker not running"             1 ""           e a '{"core":1}' ""
        expect "no docker on this machine"      1 ""           h a '{"core":1}' ""
        expect "core trigger raised"            0 "trigger:core" f a '{"core":1}' "$d --config-file=/cfgA"
        expect "trigger level already started"  1 ""           f a '{"core":2}' "$d --config-file=/cfgA"
        expect "service switched on later"      1 ""           g a '{"core":1,"svc":3}' "$d --config-file=/cfgA"
        expect "service trigger raised"         0 "trigger:svc" g a '{"core":1,"svc":2}' "$d --config-file=/cfgA"
        expect "onEnable service switched on"   0 "trigger:svc" g a '{"core":1,"svc":0}' "$d --config-file=/cfgA"
        expect "trigger missing from baseline"  1 ""           g a '{"core":1}' "$d --config-file=/cfgA"
        expect "no baseline (old system)"       1 ""           f a 'null' "$d --config-file=/cfgA"
        rm base.json
        set +e
        NEO_RESTART_RUNNING=f NEO_RESTART_BOOTED=a NEO_RESTART_BASELINE=missing.json NEO_RESTART_DOCKERD_CMDLINE="$d --config-file=/cfgA" bash "$script" f >/dev/null
        rc=$?
        set -e
        if [ "$rc" != 1 ]; then echo "FAIL no baseline file: exit $rc" >&2; fails=1; else echo "ok   no baseline file"; fi

        # The baseline an activation records (restart-baseline.jq).
        base() { jq -nc --slurpfile new "$1/neo-restart.json" --argjson base "$2" -f ${../modules/core/restart-baseline.jq}; }
        check_base() {
          if [ "$(base "$2" "$3")" != "$4" ]; then echo "FAIL baseline $1: $(base "$2" "$3") (want $4)" >&2; fails=1; else echo "ok   baseline $1"; fi
        }
        mksys j 6.18.63 "quiet" /cfgA '{"core": {"level": 1, "reason": "C.", "onEnable": false}, "on": {"level": 2, "reason": "O.", "onEnable": true}}'
        check_base "first activation of a boot" g null '{"core":1,"svc":3}'
        check_base "trigger raised keeps level" f '{"core":1}' '{"core":1}'
        check_base "service switched on"        g '{"core":1}' '{"core":1,"svc":3}'
        check_base "service switched off"       a '{"core":1,"svc":3}' '{"core":1}'
        check_base "onEnable switched on"       j '{"core":1}' '{"core":1,"on":0}'
        check_base "onEnable at boot"           j null '{"core":1,"on":2}'

        [ "$fails" = 0 ]
        touch $out
      '';
  };
}
