build:
	git add *.nix
	nix build .#nixosConfigurations.homeserver.config.system.build.vm

launch:
	just build
	pkill qemu-system-x86_64 || true
	QEMU_NET_OPTS="hostfwd=tcp::2222-:22" ./result/bin/run-nixos-vm &

exec COMMAND:
	ssh -i tools/id_ed25519 -p 2222 -o StrictHostKeyChecking=no root@localhost "{{COMMAND}}"

logs SERVICE:
	ssh -i tools/id_ed25519 -p 2222 -o StrictHostKeyChecking=no root@localhost "journalctl -u {{SERVICE}}"

ssh:
	ssh -i tools/id_ed25519 -p 2222 -o StrictHostKeyChecking=no root@localhost
