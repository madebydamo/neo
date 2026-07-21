qemu_monitor_host := "127.0.0.1"
qemu_monitor_port := "4444"
qemu_monitor := qemu_monitor_host + ":" + qemu_monitor_port
ssh_opts := "-i tools/id_ed25519 -p 2222 -o StrictHostKeyChecking=no root@localhost"
disk_image := "nixos.qcow2"

build:
  #!/usr/bin/env bash
  set -euo pipefail
  echo $(pwd)
  if [ ! -f ./settings.toml ]; then
    PWD=$(pwd)
    CONFIG_PATH="${PWD}/build"
    NEO_INPUT="git+file:${PWD}"
    printf '[neo-service]\nenabled = true\nbootstrapEnabled = true\nautoUpdateEnabled = false\n[neo-cli]\nconfigPath = "%s"\nneoInput = "%s"\ntemplate = "%s#homeserver"\n' \
      "$CONFIG_PATH" "$NEO_INPUT" "$NEO_INPUT" > settings.toml
    git add settings.toml
  fi
  nix run '.#neo' nuke
  nix run '.#neo' init
  nix run '.#neo' build

# Shut down the VM via QEMU monitor, falling back to pkill.
# Waits until the qcow2 disk is fully released before returning.
shutdown:
  #!/usr/bin/env bash
  set -euo pipefail
  # Send quit via QEMU monitor (ignoring exit code since the connection
  # drops when QEMU terminates). Falls back to pkill if monitor is not up.
  echo "quit" | nc -w 2 {{qemu_monitor_host}} {{qemu_monitor_port}} >/dev/null 2>&1 \
    || pkill -f "qemu-system.*-name nixos" 2>/dev/null \
    || { echo "No running VM found"; exit 0; }
  # Wait for the disk image to be released (up to 30s)
  echo -n "Waiting for disk release"
  for i in $(seq 1 30); do
    if qemu-img info "{{disk_image}}" >/dev/null 2>&1; then
      echo " done"
      exit 0
    fi
    echo -n "."
    sleep 1
  done
  echo " timed out - force killing"
  pkill -9 -f "qemu-system.*-name nixos" 2>/dev/null || true
  sleep 2

launch: shutdown build
  #!/usr/bin/env bash
  if [ -f ./build/result/bin/run-nixos-vm ]; then
    QEMU_NET_OPTS="hostfwd=tcp::2222-:22" \
    QEMU_OPTS="-smp 4 -m 8G -monitor tcp:{{qemu_monitor}},server,nowait" \
    ./build/result/bin/run-nixos-vm &
  else
    QEMU_NET_OPTS="hostfwd=tcp::2222-:22" \
    QEMU_OPTS="-smp 4 -m 8G -monitor tcp:{{qemu_monitor}},server,nowait" \
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
  echo "== Disk Image =="
  qemu-img info "{{disk_image}}" 2>&1 | head -5

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

