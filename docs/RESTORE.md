# Move or restore a machine

A runbook for bringing a Neo machine back on new hardware from two things only:

- the **config repo** (`/var/neo/DATA/AppData/configuration`, secrets encrypted), and
- a copy of **`/var/neo/DATA`** (your backup, or the old machine if it still runs).

Nothing else from the old machine is used: no `/var/lib`, no `/etc`, no home directories. Moving a running machine and restoring a dead one from backups are the same procedure. When the old machine is alive, `/var/neo/DATA` is pulled from it directly instead of from a backup.

It is written so a person or an AI coding agent can follow it step by step. The examples follow a typical move: an old server (say an ext4 VPS) to a new one (say a rented bare-metal machine with one NVMe that runs another OS today), possibly with a new domain on the way. Without a firmware boot entry such a move installs fine and then never boots (see [fact 5](#before-you-start-check-these-facts)); with one ([step 8](#8-boot-entry-test-boot-cutover), built into Neo) the downtime is a few minutes. The background is in [INSTALL.md — Rebuild from the config repo](INSTALL.md#rebuild-a-machine-from-its-config-repo).

## The short way: `neo restore`

Everything this page does by hand is built into `neo restore` now ([design](design/backup-restore.md)). With the backup service configured in the repo ([INSTALL.md — Backups](INSTALL.md#backups)):

```bash
# from a laptop with Nix; the target runs any Linux with root SSH, or a NixOS installer
nix run github:madebydamo/neo#neo -- restore git@github.com:you/homeserver-config.git \
  --host root@NEW --disk /dev/disk/by-id/nvme-… --key ~/.ssh/<admin-key> --reboot
```

| Manual step below | `neo restore` |
|-------------------|---------------|
| 2. kexec with nixos-anywhere (placeholder store path) | done when the target is not a NixOS installer |
| 3. hardware config | generated on the target and committed (`--keep-hardware` to skip) |
| 4. repo, key, tokens on the installer | copied into the installer's RAM, with every flake input (`nix flake archive`, fetched with your laptop's credentials); tokens from `secrets.sops.yaml` to Nix; the key is deleted at the end |
| 5. Disko, ARC cap | the config's own `diskoScript`, ARC capped |
| 6. copy DATA, fix ownership | from the configured backup (rsync, restic or `zfs send`): the first destination by **Restore order** that can be read, the next one if copying fails; Hermes re-owns its data itself |
| 7. `neo restore` | the same, as the unit `neo-restore` on the target |
| 8. boot entry | Neo's boot loader install adds it; `neo restore` checks it |

No config repo left? With an encrypted (or plain files) backup, the repo is inside it, and your admin key opens it:

```bash
nix run github:madebydamo/neo#neo -- restore --from-backup user@backup.example.com:myhome \
  --host root@NEW --disk /dev/disk/by-id/nvme-… --key ~/.ssh/<admin-key> --reboot
```

`neo backup extract-config user@backup.example.com:myhome` only copies the repo out (push it somewhere first). For a [backup store](INSTALL.md#backup-store) the location is `https://<name in the store>@backup.example.com/<folder>`; your key must be in that server's list on the store.

`--check` prints the plan and changes nothing. The checklist below still applies: DNS, local plugins (refused when missing), the provider's boot order. A cutover from a machine that still runs is not automatic: stop the old one (step 6, "Old machine still running") and let its last backup run first, or use `--data-from` with a final copy.

The rest of this page is the manual procedure from the real move, kept because it explains each step and helps when something fails halfway.

## What you need

| Thing | Why |
|-------|-----|
| An ed25519 SSH key listed in `core.ssh.authorizedKeys` when the repo was last saved | decrypts `secrets.sops.yaml` and the escrowed host key `identity.sops.yaml` |
| `identity.sops.yaml` **committed** in the repo | without it the new machine gets a new host key and cannot decrypt `secrets.sops.yaml` |
| SSH as root to the target (any Linux with kexec, or a NixOS installer) | nixos-anywhere boots the installer |
| A copy of `/var/neo/DATA` | all service state; the repo holds configuration only |
| A laptop with Nix (x86_64-linux) | runs nixos-anywhere |

## Before you start: check these facts

An agent should collect these first and stop if one is surprising.

1. **Repo state.** `git status` in the config repo. Restore clones **HEAD**: uncommitted changes (for example a fresh *Encrypt* in Settings → Secrets) are not restored. Commit them in your copy of the repo.
2. **Escrow matches.** `sops -d identity.sops.yaml` with your key (`SOPS_AGE_SSH_PRIVATE_KEY_FILE=~/.ssh/<key>`): its `ssh_host_ed25519_key_pub` must equal `core.secrets.hostKey` in `settings.toml`. `neo restore` refuses otherwise.
3. **Plugins.** Look at `core.plugins`:
   - `github:` plugins in private repos need `core.git.tokens` (usually in `secrets.sops.yaml`). Nix needs them as `access-tokens` for **every** evaluation, disko included.
   - `git+file:/abs/path` plugins (for example `git+file:/var/neo/DATA/AppData/override`) only exist where that path exists. They live inside DATA, so the flake **cannot be evaluated on your laptop** or on a bare installer until that directory is put back at the same absolute path.
   - Read what each local plugin does. On the move, the `override` plugin only forced legacy-BIOS GRUB on `/dev/sda` for the old VPS; on the UEFI target `nixos-install` then failed with `cannot find a GRUB drive for /dev/sda`. Hardware-specific plugins must be dropped or changed for the new machine (remove the URL from `core.plugins` **and** its `pluginN` input in `flake.nix`, then `nix flake lock`).
4. **Target disk.** `lsblk` and `ls -l /dev/disk/by-id/` on the target. Use the `by-id` name for `disko.mainDisk`: it survives device renumbering. The disk is wiped.
5. **Firmware, boot order, network.** On the old OS, **before anything is wiped**: `ls /sys/firmware/efi` (UEFI or BIOS; Neo's Disko layout boots both) and `efibootmgr -v`. Some hosters ship an order like `PXE, built-in EFI Shell, Ubuntu (\EFI\ubuntu\shimx64.efi on the old ESP)`, and even the old OS is started through PXE (`BootCurrent` = the PXE entry). Disko makes a new ESP with a new partition GUID, so the old OS's entries point nowhere, the PXE hand-over finds no loader, and the firmware stops at the EFI Shell prompt: no network, no ping, nothing to debug without a console. Without Disko the old ESP survives and the old entries still work, which is why plain installs boot there. An install without the NixOS entry fails exactly like this. Neo's boot loader install now adds a **NixOS** entry first in the boot order and a `startup.nsh` for the EFI Shell; [step 8](#8-boot-entry-test-boot-cutover) checks it before the reboot. Also note the network driver (`ethtool -i <iface>`, e.g. `igb`) for SSH in the initrd. The default network setup is DHCP; a target that needs a static IP needs it in the config before install.
6. **DNS.** SWAG uses HTTP validation: the domain **and every subdomain** (or the `*.domain` wildcard) must resolve to the new machine, or no certificates are issued.
7. **Native service users.** Services that run as their own NixOS user (Hermes today) get their uid/gid allocated on the new machine, and the allocation file (`/var/lib/nixos/uid-map`) is not part of DATA. Note the old ids (`getent passwd hermes`, `getent group hermes`). The new ids can differ, so their data must be re-owned after the copy ([step 6](#fix-ownership-of-native-service-users)).
8. **Reading DATA as root.** Database files belong to container uids. On the old machine, `homeserver` is in the `docker` group, which is enough to read everything through a container, without a sudo password.

## 1. Prepare the repo copy (laptop)

```bash
mkdir -p ~/neo-move && cd ~/neo-move
ssh homeserver@OLD 'tar -C /var/neo/DATA/AppData -cf - --exclude=configuration/result configuration' | tar -xf -
cd configuration
git add -A && git commit -m "Pending changes before move"     # only if git status showed some
```

Change what should differ on the new machine. Here: the domain, and Disko on.

```toml
[disko]
enabled = true
mainDisk = "/dev/disk/by-id/nvme-SAMSUNG_..."

[services.swag]
domain = "dev.example.com"
```

Keep `core.hostname` unless you want to rename. Commit.

## 2. Boot the installer with nixos-anywhere

nixos-anywhere evaluates the flake **on the laptop**. With a `git+file:` plugin from DATA (fact 3) that fails, so use it only for the kexec phase. It still insists on `--flake` or `--store-paths`, so give it a dummy store path:

```bash
mkdir -p /tmp/fake-system && printf x86_64-linux > /tmp/fake-system/system
P=$(nix store add-path /tmp/fake-system --name fake-system)
nix run github:nix-community/nixos-anywhere -- --phases kexec --store-paths $P $P --target-host root@NEW
ssh-keygen -R NEW
```

The target now runs a NixOS installer from RAM (`/` and the Nix store are a tmpfs of half the RAM); its disk is untouched so far. The installer has no `git`, and `/etc/hosts` is read-only (use IPs). After Disko, the ZFS ARC competes with that tmpfs for RAM: with a big copy running, `nix` died with SIGBUS (exit 135) while building. Cap the ARC right after step 5: `echo $((4*1024**3)) > /sys/module/zfs/parameters/zfs_arc_max`.

If your config has no `git+file:` plugins, you can skip steps 2–5 and run plain nixos-anywhere instead ([alternative](#alternative-one-nixos-anywhere-run)).

## 3. Hardware config

The old `hardware-configuration.nix` describes the old machine (for example a QEMU guest with an ext4 root) and would clash with Disko. Generate the new one **without filesystems** (Disko owns them) and commit it:

```bash
ssh root@NEW 'nixos-generate-config --no-filesystems --show-hardware-config' > configuration/hardware-configuration.nix
git -C configuration commit -am "hardware-configuration: new machine"
```

On a server without a working console (IPMI), add SSH in the initrd, so a boot that stops in the initrd (ZFS import, missing driver) can be seen and fixed from outside. Add the network driver from fact 5 and:

```nix
  boot.initrd.availableKernelModules = [ "nvme" "xhci_pci" "ahci" "igb" ];
  boot.initrd.network = {
    enable = true;
    ssh = {
      enable = true;
      port = 2222;
      authorizedKeys = config.neo.core.ssh.authorizedKeys;
      hostKeys = [ "/etc/secrets/initrd/ssh_host_ed25519_key" ];  # created in step 5
    };
  };
  boot.initrd.systemd.network.networks."10-uplink" = {
    matchConfig.Name = "en*";
    networkConfig.DHCP = "yes";
  };
```

During the reboot, `ping` without port 2222 means firmware or GRUB; port 2222 open but 22 not means the initrd.

## 4. Put the repo, key and local plugins on the installer

```bash
tar -C ~/neo-move -cf - configuration | ssh root@NEW 'mkdir -p /root/restore && tar -C /root/restore -xf - && mv /root/restore/configuration /root/restore/config && chown -R root:root /root/restore'
ssh root@NEW 'install -d -m700 /root/.ssh && cat > /root/.ssh/neo-admin && chmod 600 /root/.ssh/neo-admin' < ~/.ssh/<admin-key>
ssh root@NEW 'nix profile install nixpkgs#git && git config --global --add safe.directory "*"'
```

The key only lives in the installer's RAM and is gone after the reboot.

`git+file:` plugins: copy them from DATA to **the same absolute path on the installer** (not under `/mnt`):

```bash
ssh homeserver@OLD 'tar -C /var/neo/DATA/AppData -cf - override' | ssh root@NEW 'mkdir -p /var/neo/DATA/AppData && tar -C /var/neo/DATA/AppData -xf -'
```

Private GitHub plugins: write the decrypted token into a Nix config file on the installer (never print it):

```bash
ssh root@NEW 'cd /root/restore/config && SOPS_AGE_SSH_PRIVATE_KEY_FILE=/root/.ssh/neo-admin \
  nix --extra-experimental-features "nix-command flakes" shell nixpkgs#sops nixpkgs#yq-go -c sh -c \
  "printf \"experimental-features = nix-command flakes\naccess-tokens = github.com=%s\n\" \"\$(sops -d secrets.sops.yaml | yq .core.git.tokens.\\\"github.com\\\")\" > /root/restore/nix.conf" \
  && chmod 600 /root/restore/nix.conf'
```

Check that the flake evaluates and Disko points at the right disk **before** wiping:

```bash
ssh root@NEW 'cd /root/restore/config && export NIX_CONFIG="$(cat /root/restore/nix.conf)"
  nix eval --raw .#nixosConfigurations.neo.config.disko.devices.disk.main.device; echo
  nix eval --raw .#nixosConfigurations.neo.config.neo.services.swag.domain'
```

## 5. Disko (wipes the disk)

```bash
ssh root@NEW 'export NIX_CONFIG="$(cat /root/restore/nix.conf)"
  nix run github:nix-community/disko/latest -- --yes-wipe-all-disks --mode destroy,format,mount --flake /root/restore/config#neo
  zfs list; findmnt -R /mnt'
```

Expected: `zroot/root` on `/mnt`, `zroot/neo` on `/mnt/var/neo`, the ESP on `/mnt/boot`. With initrd SSH (step 3), create its host key now, before the install copies it into the initrd:

```bash
ssh root@NEW 'install -d -m700 /mnt/etc/secrets/initrd && ssh-keygen -q -t ed25519 -N "" -f /mnt/etc/secrets/initrd/ssh_host_ed25519_key'
```

## 6. Copy `/var/neo/DATA`

Pull from the old machine (or your backup host) **to** the new one, as root on both sides. Leave out `AppData/configuration`: `neo restore` creates it and refuses a non-empty directory.

On the old machine, run an **rsync daemon in a container** (the container reads every file as root; `homeserver` only needs the `docker` group), bound to localhost, and reach it through an SSH tunnel. Do not use `--rsync-path="docker run -i …"`: every byte then goes through `dockerd`, which capped the move at 2–5 MB/s; the daemon did 88 MB/s on the same machines.

```bash
# old machine: an rsync binary from its Nix store, then the daemon
ssh homeserver@OLD 'R=$(nix --extra-experimental-features "nix-command flakes" build --no-link --print-out-paths nixpkgs#rsync)/bin/rsync
  docker run -d --name neo-move-rsyncd --network host -v /nix/store:/nix/store:ro -v /var/neo/DATA:/var/neo/DATA:ro alpine:latest \
    sh -c "printf \"uid = root\ngid = root\nuse chroot = no\nnumeric ids = yes\n[data]\npath = /var/neo/DATA\nread only = yes\n\" > /rsyncd.conf; exec $R --daemon --no-detach --address=127.0.0.1 --port=8873 --config=/rsyncd.conf"'

# new machine: the tunnel as a unit (the admin key from step 4 logs in to the old machine too)
ssh root@NEW 'systemd-run --unit=neo-tunnel -E HOME=/root /run/current-system/sw/bin/ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 \
  -i /root/.ssh/neo-admin -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new -L 18873:127.0.0.1:8873 homeserver@OLD_IP'
```

`/root/restore/sync-d.sh` on the installer (re-runnable; later runs copy only the delta; an argument limits it to one directory):

```sh
#!/bin/sh
set -e
P="${1:-}"
mkdir -p "/mnt/var/neo/DATA/$P"
rsync -aH --numeric-ids --delete --info=stats2 \
  --exclude=/AppData/configuration/ \
  "rsync://127.0.0.1:18873/data/$P/" "/mnt/var/neo/DATA/$P/"
```

Run long jobs as **systemd units on the installer**, never in a plain SSH session: they then survive a dropped connection or a closed laptop. Always raise the file limit (the default 1024 breaks `nixos-install`):

```bash
ssh root@NEW 'systemd-run --unit=neo-sync -p LimitNOFILE=1048576 -E PATH=/run/current-system/sw/bin -E HOME=/root sh -c "/root/restore/sync-d.sh > /root/restore/sync.log 2>&1"'
```

Progress: `zfs get logicalused zroot/neo` (plain `du` shows the zstd-compressed size and looks far too small). Check afterwards that nothing is left: `rsync -aHn --numeric-ids --delete --itemize-changes --exclude=/AppData/configuration/ rsync://127.0.0.1:18873/data/ /mnt/var/neo/DATA/ | wc -l` should print 0 (re-owned directories, see below, show up as differences).

**Old machine still running:** do a first pass live, then stop its services, run `sync-d.sh` again for a consistent delta (seconds to minutes), and leave them stopped so two machines don't run the same Telegram bot or crawlers. Stop them through a transient unit on the old machine, not in the SSH session (an SSH session that stopped Docker units dropped before the command ran); `homeserver` may run `systemctl` and `systemd-run` through sudo without a password:

```bash
ssh homeserver@OLD 'sudo systemctl stop neo-auto-update.timer neo-docker-updater.timer
  U=$(systemctl list-units --no-legend --plain "docker-*.service" | awk "{print \$1}")
  sudo systemd-run --unit=neo-move-stop /run/current-system/sw/bin/systemctl stop hermes-agent.service hermes-dashboard.service neo-web.service $U'
# back on (if the move has to wait): sudo systemd-run --unit=neo-move-start /run/current-system/sw/bin/systemctl start multi-user.target timers.target
```

A reboot of the old machine starts everything again; power it off or reinstall it once the new one works. Afterwards: `docker rm -f neo-move-rsyncd` on the old machine.

Things that went wrong on the real move:

- Do not use `tar` for the first pass and kill it: GNU tar extracts symlinks as empty placeholder files and fixes them only at the very end. A killed tar leaves placeholders, and Hermes' activation then aborts the whole system activation. The rsync above repairs them.
- `pkill -f "<pattern>"` inside `ssh host '…'` also matches that SSH session's own shell and kills it.
- Killing the local rsync leaves the sender running in its container on the old machine: `docker kill` it there.

### Fix ownership of native service users

After the last sync, re-own the data of services that run as their own NixOS user (fact 7). The new ids are in `/mnt/etc/passwd` after the first `nixos-install` run (step 7); if you copy DATA before installing, re-own after the install and before the reboot. Example from the move (Hermes 995:992 → 998:998; on the new machine 995:992 belonged to `systemd-oom`):

```bash
ssh root@NEW 'H=/mnt/var/neo/DATA/AppData/hermes
  find $H -xdev -user 995 -exec chown -h 998 {} +
  find $H -xdev -group 992 -exec chgrp -h 998 {} +'
```

Containers are not affected: they use `homeserver` (1000, fixed) or ids inside the image.

## 7. `neo restore`

```bash
ssh root@NEW 'systemd-run --unit=neo-restore -p LimitNOFILE=1048576 -p WorkingDirectory=/root \
  -E PATH=/run/current-system/sw/bin:/root/.nix-profile/bin -E HOME=/root \
  sh -c "nix --extra-experimental-features \"nix-command flakes\" run github:madebydamo/neo#neo -- restore /root/restore/config --key /root/.ssh/neo-admin > /root/restore/restore.log 2>&1"'
ssh root@NEW 'journalctl -fu neo-restore'    # or tail -f /root/restore/restore.log
```

It clones the repo to `/mnt/var/neo/DATA/AppData/configuration`, writes the old SSH host key to `/mnt/etc/ssh` ("✓ Host key restored"), passes `core.git.tokens` to Nix and runs `nixos-install`. The first run builds the `neo` CLI and every plugin from source; count on 30+ minutes.

The `git+file:` plugin path from step 4 must still exist on the installer at that point.

To rerun only the install after a failure (restore refuses a non-empty config directory):

```bash
ssh root@NEW 'export NIX_CONFIG="$(cat /root/restore/nix.conf)"
  nixos-install --root /mnt --no-root-passwd --flake /mnt/var/neo/DATA/AppData/configuration#neo'
```

or `rm -rf /mnt/var/neo/DATA/AppData/configuration /mnt/etc/ssh` and run `neo restore` again.

`neo restore` sets the restored repo's `origin` to the URL you gave it. With a local path, fix it afterwards: `git -C /mnt/var/neo/DATA/AppData/configuration remote remove origin` (or set your real remote).

## 8. Boot entry, test boot, cutover

**Boot entry (UEFI).** Before the first reboot, check on the installer that `nixos-install` left a **NixOS** entry first in `BootOrder`, pointing at the new ESP:

```bash
ssh root@NEW 'nix profile install nixpkgs#efibootmgr; efibootmgr -v; ls /mnt/boot/startup.nsh'
```

The install log shows `neo-boot: adding firmware boot entry NixOS …`. Stale entries of the old OS can stay: they come after it. The entry stays in NVRAM, and later switches leave its position alone. With the entry first, the hoster's PXE no longer runs before the OS: for a reinstall from the hoster's panel, switch the boot order back to network (PXE) first.

Config repos that pin an older Neo (before the hybrid boot layout) don't do this. Run the manual fix from the move instead; there the ESP is partition 1:

```sh
#!/bin/sh
# /root/restore/bootfix.sh (nix profile install nixpkgs#efibootmgr first)
set -e
DISK=/dev/disk/by-id/nvme-...                       # disko.mainDisk; the ESP is partition 1
for n in $(efibootmgr | grep -iE '^Boot[0-9A-F]{4}\*? (ubuntu|NixOS)' | cut -c5-8); do
  efibootmgr -q -b "$n" -B
done
efibootmgr -q -c -d "$DISK" -p 1 -L NixOS -l '\EFI\BOOT\BOOTX64.EFI'
new=$(efibootmgr | grep -E '^Boot[0-9A-F]{4}\*? NixOS' | cut -c5-8 | head -1)
rest=$(efibootmgr | sed -n 's/^BootOrder: //p' | tr ',' '\n' | grep -v "^$new\$" | paste -sd, -)
efibootmgr -q -o "$new${rest:+,$rest}"
printf '@echo -off\r\nfor %%d in fs0 fs1 fs2 fs3 fs4 fs5\r\n  if exist %%d:\\EFI\\BOOT\\BOOTX64.EFI then\r\n    %%d:\\EFI\\BOOT\\BOOTX64.EFI\r\n  endif\r\nendfor\r\n' > /mnt/boot/startup.nsh
efibootmgr -v
```

**Test boot while the old machine still runs.** Then a broken boot costs nothing but time:

```bash
ssh root@NEW 'rm -f /root/.ssh/neo-admin /root/restore/nix.conf; sync; umount -R /mnt; zpool export zroot
  systemd-run --on-active=3 /run/current-system/sw/bin/systemctl reboot'
ssh-keygen -R NEW
# watch: ping, port 2222 (initrd), port 22 (system). Bare metal with slow firmware: SSH after about 3 minutes.
ssh homeserver@NEW 'hostname; findmnt / /var/neo; systemctl is-system-running'
```

The new machine starts every service on the copied data, including Hermes with the same Telegram token: stop it right away (`sudo systemctl stop hermes-agent hermes-dashboard`), or two bots poll at once (a cron job sent one message in those seconds).

**Cutover.**

1. On the new machine stop everything, so the final copy doesn't land under running services: `sudo systemctl stop neo-auto-update.timer neo-docker-updater.timer`, then `sudo systemd-run --unit=neo-move-stop systemctl stop hermes-agent.service hermes-dashboard.service neo-web.service <every docker-*.service>`.
2. Stop the old machine the same way ([step 6](#6-copy-varneodata)).
3. Final delta into the running system. `homeserver` can't read DATA, but may run `systemd-run` as root; the tunnel to the old machine's rsync daemon uses your forwarded agent for these few minutes:

   ```bash
   ssh -A homeserver@NEW 'ssh -f -N -o ExitOnForwardFailure=yes -L 18873:127.0.0.1:8873 homeserver@OLD_IP
     sudo systemd-run --wait --unit=neo-final-sync -p LimitNOFILE=1048576 /run/current-system/sw/bin/rsync -aH --numeric-ids --delete \
       --exclude=/AppData/configuration/ rsync://127.0.0.1:18873/data/ /var/neo/DATA/'
   ```

   (531 files, 159 MB, under a minute on the move.) Units started with `systemd-run` get no `PATH`: pass `-E PATH=/run/current-system/sw/bin` to anything that runs a shell.
4. Re-own native service users again (the final sync restored the old ids), now on `/var/neo/...` and with `id -u hermes`.
5. `docker rm -f neo-move-rsyncd` on the old machine, reboot the new one (a second boot test), and run the smoke test from `AGENTS.md`: units active, SWAG without `emerg`, `https://<sub>.<domain>/` answers 302 to tinyauth.

The host key is the old one, so clients that knew the old machine by name already trust it; the IP is new, so `ssh-keygen -R NEW` the IP once. A changed domain makes SWAG request a new certificate on first start (and revoke the old one), so DNS must point at the new machine by then.

What does **not** come back, because it is not in DATA:

- the machine git key (`/var/lib/neo/git/id_ed25519`) and `homeserver`'s own `~/.ssh/id_ed25519`: both are regenerated. Add the new git key as a deploy key wherever the old one was used (`core.git.useHostKey` uses the restored host key instead).
- Docker images: pulled again on first start.
- Journal, `neo-web` version notes.

## Alternative: one nixos-anywhere run

When the flake evaluates on the laptop (no `git+file:` plugins, or that path recreated on the laptop), nixos-anywhere can do Disko and the install in one go. The host key and the config repo go in with `--extra-files`; DATA follows before the first boot:

```bash
mkdir -p extra/etc/ssh extra/var/neo/DATA/AppData
sops -d --extract '["ssh_host_ed25519_key"]' configuration/identity.sops.yaml > extra/etc/ssh/ssh_host_ed25519_key
chmod 600 extra/etc/ssh/ssh_host_ed25519_key
cp -a configuration extra/var/neo/DATA/AppData/configuration
NIX_CONFIG="access-tokens = github.com=…" nix run github:nix-community/nixos-anywhere -- \
  --flake ./configuration#neo --phases kexec,disko,install --extra-files ./extra --chown /var/neo/DATA/AppData/configuration 1000:1000 \
  --target-host root@NEW
# then step 6 (copy DATA into /mnt, fix ownership) and step 8 (reboot)
```

This builds on the laptop and skips `neo restore`'s checks (escrow matches `core.secrets.hostKey`); do that check by hand.

## Known gaps (for Neo developers)

Fixed since the move:

- **Disko boot.** `neo.disko` was UEFI-only and relied on the firmware finding `\EFI\BOOT\BOOTX64.EFI` by itself: on firmware that never tries that path it did not boot, and on legacy-BIOS machines it could not boot at all, so those needed a hand-written `EF02` + ext4 layout. The layout now has a 1 MB `EF02` partition, GRUB goes into the MBR as well, and the boot loader install keeps a NixOS firmware entry and `startup.nsh` (`nix/modules/disko/boot-install.sh`). `just test-disko-boot` boots the layout under OVMF and SeaBIOS.
- **Manual restore.** `neo restore` does Disko, the data and the install, also from a client (`--host`).
- **Hermes uid.** Its activation re-owns its appdata when the uid changed (`hermes-reown`).
- **Backup key.** The backup logs in with the escrowed host key by default, so it survives a restore.

Open:

- Machine-specific settings (boot loader, hardware) can hide in local plugins and nothing flags them on restore; `neo restore` could print the effective `boot.loader` and warn when it doesn't match the installer (EFI vs BIOS).
- `git+file:` plugins that live in DATA: `neo restore` refuses them (the flake must evaluate before the data is back). Put such plugins into git.
- Cutover from a running machine is manual: stop the old one, let its backup run, then restore.
- The `hermes-neo-skills` activation snippet uses `exit 1`, which ends the whole activation script (later snippets and `/run/current-system` are skipped), not just that snippet.
- Encrypting in Settings → Secrets moved only options declared with `mkSecretOption`; several plugin API keys and session secrets stayed in plain text in `settings.toml`. Check before pushing the repo anywhere public.
