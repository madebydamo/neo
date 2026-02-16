qemu_monitor_host := "127.0.0.1"
qemu_monitor_port := "4444"
qemu_monitor := qemu_monitor_host + ":" + qemu_monitor_port
ssh_opts := "-i tools/id_ed25519 -p 2222 -o StrictHostKeyChecking=no root@localhost"
disk_image := "nixos.qcow2"

build:
  git add *.nix
  nix build .#nixosConfigurations.homeserver.config.system.build.vm

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
  QEMU_NET_OPTS="hostfwd=tcp::2222-:22" \
  QEMU_OPTS="-monitor tcp:{{qemu_monitor}},server,nowait" \
  ./result/bin/run-nixos-vm &

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
  ssh {{ssh_opts}} "journalctl -u {{SERVICE}}"

ssh:
  ssh {{ssh_opts}}

format:
  alejandra .
