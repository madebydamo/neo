qemu_monitor_host := "127.0.0.1"
qemu_monitor_port := "4444"
qemu_monitor := qemu_monitor_host + ":" + qemu_monitor_port
# Dev VM root shell (vm module: key-only PermitRootLogin). Real installs stay admin/homeserver.
ssh_opts := "-i tools/development_ed25519 -p 2222 -o StrictHostKeyChecking=no root@localhost"
# Ext4 dev VM. The disko/ZFS VM must never open or replace this image.
disk_image := "nixos.qcow2"
# Persistent qcow2 chain for the disko VM (base/ + overlay). See vm-configuration.nix.
disko_disk_dir := "nixos-disko"

build:
  #!/usr/bin/env bash
  set -euo pipefail
  echo $(pwd)
  if [ ! -f ./settings.toml ]; then
    PWD=$(pwd)
    # local follows this checkout. server.neoInput stays the default
    # github:madebydamo/neo so a guest init does not use this path.
    printf '[neo-cli.local]\nconfigPath = "%s/build"\nneoInput = "git+file:%s"\ntemplate = "%s#homeserver"\n\n[services.system-updater]\nenabled = false\n' \
      "$PWD" "$PWD" "$PWD" > settings.toml
    git add settings.toml
  fi
  nix run '.#neo' nuke
  nix run '.#neo' init
  nix run '.#neo' build

# Shut down the VM via QEMU monitor, falling back to pkill.
# Waits until QEMU has exited so both nixos.qcow2 and nixos-disko/ are released.
# Does not delete either image.
shutdown:
  #!/usr/bin/env bash
  set -euo pipefail
  # [q] so this script does not match itself.
  qemu_pat='[q]emu-system.*-name nixos'
  if ! pgrep -f "$qemu_pat" >/dev/null 2>&1; then
    echo "No running VM found"
    exit 0
  fi
  # The monitor connection drops when QEMU terminates.
  echo "quit" | nc -w 2 {{qemu_monitor_host}} {{qemu_monitor_port}} >/dev/null 2>&1 || true
  echo -n "Waiting for VM to exit"
  for i in $(seq 1 30); do
    if ! pgrep -f "$qemu_pat" >/dev/null 2>&1; then
      echo " done"
      exit 0
    fi
    echo -n "."
    sleep 1
  done
  echo " timed out - force killing"
  pkill -9 -f "$qemu_pat" 2>/dev/null || true
  sleep 2

# virtfs for local plugin checkouts. Tag is "h" plus the first 8 hex chars of
# sha256(path), the same as shareTag in nix/output/device/vm-configuration.nix.
host_virtfs:
  #!/usr/bin/env python3
  import hashlib
  import pathlib
  import tomllib

  candidates = [pathlib.Path("build/settings.toml"), pathlib.Path("settings.toml")]
  settings = next((p for p in candidates if p.exists()), None)
  plugins = []
  if settings is not None:
      data = tomllib.loads(settings.read_text())
      plugins = (data.get("core") or {}).get("plugins") or []

  dirs = []
  for url in plugins:
      if not isinstance(url, str):
          continue
      base = url.split("#", 1)[0]
      for prefix in ("git+file://", "git+file:", "path:"):
          if base.startswith(prefix):
              base = base[len(prefix):]
              break
      else:
          if not base.startswith("/"):
              continue
      if base:
          dirs.append(base.rstrip("/"))

  roots = []
  for path in sorted(set(dirs)):
      if any(path != other and path.startswith(other + "/") for other in dirs):
          continue
      if "," in path:
          raise SystemExit("plugin path cannot contain a comma: " + path)
      roots.append(path)

  for path in roots:
      tag = "h" + hashlib.sha256(path.encode()).hexdigest()[:8]
      print("-virtfs local,path=%s,security_model=none,mount_tag=%s,id=%s" % (path, tag, tag))

launch: shutdown build
  #!/usr/bin/env bash
  shares=$(just host_virtfs)
  # Host libs (e.g. alsa-lib) built against another glibc break QEMU.
  unset LD_LIBRARY_PATH
  if [ -f ./build/result/bin/run-nixos-vm ]; then
    QEMU_NET_OPTS="hostfwd=tcp::2222-:22" \
    QEMU_OPTS="-smp 4 -m 8G -monitor tcp:{{qemu_monitor}},server,nowait ${shares}" \
    ./build/result/bin/run-nixos-vm &
  else
    QEMU_NET_OPTS="hostfwd=tcp::2222-:22" \
    QEMU_OPTS="-smp 4 -m 8G -monitor tcp:{{qemu_monitor}},server,nowait ${shares}" \
    DISKO_VM_STATE_DIR="$PWD/{{disko_disk_dir}}" \
    ./build/result/bin/disko-vm &
  fi

web:
  #!/usr/bin/env bash
  set -euo pipefail
  SETTINGS="build/settings.toml"
  if [ ! -f "$SETTINGS" ]; then
    echo "build/settings.toml not found."
    echo "Run 'just build' (or 'just launch') once to initialize the build directory."
    echo "After initial setup, 'just web' uses fast rebuilds (nix run .#neo) against the existing build/ only."
    exit 1
  fi
  echo "neo web (dev from source tree) -> $SETTINGS"
  echo "Edit files under cli/; the server will auto-recompile + restart. Ctrl-C to stop."
  echo "Tip: 'cd build && nix flake update neo' to refresh Nix module/option definitions from source."
  WATCH_PATHS=(cli/src cli/templates cli/static cli/web-css cli/Cargo.toml cli/Cargo.lock)
  get_mtime() {
    find "${WATCH_PATHS[@]}" -type f -exec stat -c %Y {} + 2>/dev/null | sort -n | tail -1 || echo 0
  }
  cleanup() {
    if [ -n "${SERVER_PID:-}" ]; then
      kill "$SERVER_PID" 2>/dev/null || true
      wait "$SERVER_PID" 2>/dev/null || true
    fi
  }
  trap cleanup EXIT INT TERM
  while true; do
    MTIME=$(get_mtime)
    # Always build via nix so native deps (openssl, pkg-config) are provided.
    # Rebuild is a no-op (or very fast) when nothing changed; triggers on cli edits.
    nix build .#neo -o /tmp/neo-web-dev
    /tmp/neo-web-dev/bin/neo --settings "$SETTINGS" web &
    SERVER_PID=$!
    echo "[$(date +%H:%M:%S)] started (pid $SERVER_PID)"
    while kill -0 "$SERVER_PID" 2>/dev/null; do
      sleep 1
      NEW_MTIME=$(get_mtime)
      if [ "$NEW_MTIME" != "$MTIME" ]; then
        echo "[$(date +%H:%M:%S)] change in cli/ — restarting"
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
        sleep 0.2
        break
      fi
    done
    wait "$SERVER_PID" 2>/dev/null || true
    # Backoff on crash with unchanged sources (e.g. build error); next mtime change will retry immediately.
    CUR_MTIME=$(get_mtime)
    if [ "$CUR_MTIME" == "$MTIME" ]; then
      sleep 2
    fi
  done

# Show VM status: QEMU process, monitor, SSH, and disk image info.
status:
  #!/usr/bin/env bash
  echo "== QEMU Process =="
  pid=$(pgrep -f "[q]emu-system.*-name nixos" 2>/dev/null | head -1)
  if [ -n "$pid" ]; then
    echo "running (pid $pid)"
    echo ""
    echo "== Monitor =="
    echo "info status" | nc -w 2 {{qemu_monitor_host}} {{qemu_monitor_port}} 2>/dev/null \
      | tr -d '\r' | grep -o "VM status: .*" \
      || echo "not reachable"
    echo ""
    echo "== SSH =="
    ssh -o ConnectTimeout=3 {{ssh_opts}} "uptime" 2>/dev/null \
      || echo "not reachable"
  else
    echo "not running"
  fi
  echo ""
  echo "== Disk images =="
  for img in "{{disk_image}}" "{{disko_disk_dir}}"/*.qcow2; do
    if [ -f "$img" ]; then
      echo "-- $img"
      qemu-img info "$img" 2>&1 | head -6
    fi
  done

exec COMMAND:
  ssh {{ssh_opts}} "{{COMMAND}}"

logs SERVICE:
  ssh {{ssh_opts}} "journalctl -b -u {{SERVICE}}"

ssh:
  ssh {{ssh_opts}}

format:
  #!/usr/bin/env bash
  set -euo pipefail
  alejandra -e ./build/ . 2> /dev/null
  if command -v cargo >/dev/null 2>&1; then
    cargo fmt --all --manifest-path cli/Cargo.toml
  else
    nix develop --command cargo fmt --all --manifest-path cli/Cargo.toml
  fi
  echo "Rust formatted"

test-widgets:
  #!/usr/bin/env bash
  set -euo pipefail
  if command -v node >/dev/null 2>&1; then
    node --test cli/templates/options/widgets/*.test.js cli/templates/options/widgets/test/*.test.js cli/static/*.test.js
  else
    nix-shell -p nodejs --run 'node --test cli/templates/options/widgets/*.test.js cli/templates/options/widgets/test/*.test.js cli/static/*.test.js'
  fi

check:
  #!/usr/bin/env bash
  set -euo pipefail
  git add flake.nix nix/ templates/
  nix flake check
  #if [ -d build ]; then
  #  (cd build && git add . && nix flake check)
  #fi
  alejandra -c -e ./build/ . 2> /dev/null
  if command -v cargo >/dev/null 2>&1; then
    cargo fmt --all --manifest-path cli/Cargo.toml -- --check
  else
    nix develop --command cargo fmt --all --manifest-path cli/Cargo.toml -- --check
  fi

