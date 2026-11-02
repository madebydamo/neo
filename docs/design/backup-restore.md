# Backup and restore

One destination setting, two actions: the **backup** service copies Neo's data there on a
schedule, and **`neo restore`** reads the same settings from the config repo and rebuilds a
machine from repo + backup. Written after a real move ([RESTORE.md](../RESTORE.md)):
everything done by hand there is a step of `neo restore` now.

## 1. What gets backed up

`neo.core.volumes.root` (`/var/neo`): all app data and the config repo. Never the system, `/nix`,
`/var/lib` or `/etc`. The config repo in git is the source of truth for configuration; the copy in
the backup is only a fallback.

On a Disko (ZFS) machine every run first takes a snapshot `zroot/neo@neo-backup-<UTC>` and copies
**from the snapshot**, so databases are copied at one point in time, not while they change. Without
ZFS the live folder is copied (as before).

## 2. Destination (`neo.services.backup`)

| Setting | Values | Meaning |
|---------|--------|---------|
| `target` | `ssh` (default), `disk` | another computer over SSH, or a disk attached to this server |
| `format` | `files` (default), `zfs` | plain copy with rsync (any filesystem), or `zfs send` (ZFS on both sides, Disko here) |
| `host`, `user`, `sshKey`, `extraOptions` | | SSH login, `target = ssh` only |
| `remotePath` | path | `format = files`: folder. Relative paths are in the SSH user's home; on a disk it must be absolute (the disk's mount point and a folder) |
| `dataset` | `pool/path` | `format = zfs`: dataset that receives the snapshots, e.g. `tank/backup/myhome` |
| `keep` | int, default 14 | `format = zfs`: backup snapshots kept at the destination |
| `schedule`, `excludedDirs`, `sourceDir` | | as before (`excludedDirs`/`sourceDir` only for `files`) |

The four combinations:

- **ssh + files**: `rsync -aH --numeric-ids --delete` to `user@host:remotePath/<basename sourceDir>/`
  (the layout of the old backup service, so existing backups keep going).
- **disk + files**: the same into a local path. Refuses to run when that path is on the root
  filesystem (an unplugged disk would otherwise fill `/`).
- **ssh + zfs**: `zfs send [-i <last common snapshot>] zroot/neo@neo-backup-… | ssh … zfs recv -u
  -F -o canmount=noauto -o com.sun:auto-snapshot=false <dataset>`. Needs root or
  `zfs allow <user> create,receive,mount,destroy,hold <parent>` on the destination. Only the
  newest backup snapshot is kept locally (the base of the next incremental); the destination keeps
  `keep`. The first run is a full send and refuses a destination dataset that exists already
  without a common snapshot, instead of overwriting it.
- **disk + zfs**: the same, `zfs recv` locally. The pool (first part of `dataset`) is imported
  with `-N` when needed and exported again afterwards.

**SSH key.** The default is now the SSH **host key** (`/etc/ssh/ssh_host_ed25519_key`; the backup
runs as root). The host key is escrowed in `identity.sops.yaml` and restored on reinstall, so a
restored machine logs in to its backup with the same key, and `neo restore` can read the backup with
it. The old default (`homeserver`'s own key) was not in DATA and was lost with the machine.
Migration `013_backup_keeps_ssh_key` pins the old default in existing configs that enabled backup.

## 3. `neo restore`

```
neo restore <repo> [--host root@IP] [--disk /dev/disk/by-id/…] [--key ~/.ssh/admin]
                   [--data backup|none] [--keep-hardware] [--yes] [--reboot]
```

Two modes, one code path:

- **On the new machine** (no `--host`): booted from any NixOS installer, as root. Everything below
  runs locally.
- **From a client** (`--host root@IP`): a laptop with Nix. If the target is not a NixOS installer
  yet (any Linux with root SSH, e.g. the hoster's Ubuntu), **nixos-anywhere** kexecs it into one.
  The repo and the admin key are copied into the installer's RAM, and the local mode runs there as a
  systemd unit (it survives a dropped connection); its journal is streamed back. The `neo` that runs
  there is the revision pinned in the repo's `flake.lock`, so restore and the config match.

Why not one nixos-anywhere run with `--flake`: it evaluates and builds on the client, which fails
for `git+file:` plugins, needs the private-plugin tokens on the client, and has no step between
Disko and the install to put the data back before the first boot.

Steps (local mode):

1. **Clone** the repo to `/root/neo-restore/config`.
2. **Unlock**: decrypt `identity.sops.yaml` with the admin key (host key, checked against
   `core.secrets.hostKey`) and `core.git.tokens` (Nix `access-tokens`).
3. **Adapt to the machine**, committed in the repo as one commit "restore: <host>":
   `--disk` → `disko.mainDisk`; `hardware-configuration.nix` regenerated with
   `nixos-generate-config --no-filesystems` (unless `--keep-hardware`).
4. **Preflight** (refuses before wiping): Disko enabled, `mainDisk` exists, the flake evaluates,
   `git+file:` plugins exist on this machine, the backup is reachable and has data. Then it prints
   the plan (disk, data source and size, hostname, domain) and asks (unless `--yes`).
5. **Disko**: the config's own `diskoScript` (destroy, format, mount), so the Disko version is the
   one the config pins. The ZFS ARC is capped while the installer's store lives in RAM.
6. **Data** from the backup into the new `zroot/neo`: rsync of `<remotePath>/<basename>/` for
   `files`, `zfs send` of the newest backup snapshot into `zroot/neo` for `zfs` (the next backup
   from the new machine continues incrementally). The backup's copy of the config repo is moved
   aside (`configuration.from-backup`); the repo from git is installed.
7. **Install**: repo to `<configPath>`, host key to `/mnt/etc/ssh`, `nixos-install`.
8. **Check** the firmware boot entry (UEFI) and, with `--reboot`, unmount, export and reboot.

Native NixOS service users (Hermes) get their uid from the new machine; their activation re-owns
their appdata when the folder still belongs to the old uid, so the copy does not need fixing by hand.

## 4. Not covered

- Encryption at the destination (use an encrypted remote dataset or disk).
- `git+file:` plugins that live in DATA: restore refuses and names them; put them into git.
- Restores of single services from the remote backup (local ZFS snapshots cover that).
