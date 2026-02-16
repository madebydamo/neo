qemu_monitor := "127.0.0.1:4444"
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
  echo "quit" | nc -w 2 {{qemu_monitor}} >/dev/null 2>&1 \
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

exec COMMAND:
  ssh {{ssh_opts}} "{{COMMAND}}"

logs SERVICE:
  ssh {{ssh_opts}} "journalctl -u {{SERVICE}}"

ssh:
  ssh {{ssh_opts}}

format:
  alejandra .
