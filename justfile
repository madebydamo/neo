build:
	git add *.nix
	nix build .#nixosConfigurations.homeserver.config.system.build.vm

launch:
	ssh -o ConnectTimeout=5 -i tools/id_ed25519 -p 2222 -o StrictHostKeyChecking=no root@localhost "shutdown now" && sleep 5 || true
	just build
	QEMU_NET_OPTS="hostfwd=tcp::2222-:22" ./result/bin/run-nixos-vm &

exec COMMAND:
	ssh -i tools/id_ed25519 -p 2222 -o StrictHostKeyChecking=no root@localhost "{{COMMAND}}"

logs SERVICE:
	ssh -i tools/id_ed25519 -p 2222 -o StrictHostKeyChecking=no root@localhost "journalctl -u {{SERVICE}}"

ssh:
	ssh -i tools/id_ed25519 -p 2222 -o StrictHostKeyChecking=no root@localhost
