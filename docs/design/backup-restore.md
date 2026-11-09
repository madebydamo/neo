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

## 2. Destinations (`neo.services.backup`)

`destinations.<name>` lists where the backup goes; every run copies to each enabled one. Several
destinations are how a 3-2-1 backup looks here: e.g. a USB disk on the server and a friend's
computer. The same computer may appear twice with different folders (two of its disks); two
destinations writing to the same place (login + folder or dataset) are refused by assertion.

Shared by all destinations: `schedule`, `sourceDir`, `excludedDirs`. Per destination:

| Setting | Values | Meaning |
|---------|--------|---------|
| `enabled` | default true | off: skipped by the backup and by `neo restore`, its data stays |
| `target` | `ssh` (default), `disk`, `store` | another computer over SSH, a disk attached to this server, or a Neo backup store over HTTPS (§6) |
| `format` | `files` (default), `encrypted`, `zfs` | plain copy with rsync (any filesystem), restic repository encrypted on this server (§3), or `zfs send` (ZFS on both sides, Disko here) |
| `host`, `user` | | SSH login, `target = ssh` only |
| `sshKey`, `extraOptions` | | SSH key and options, `target = ssh` or `store` |
| `storeUrl`, `storeClient` | | `target = store`: the store's `https://` address, and this server's name there (default: hostname) |
| `remotePath` | path | `files`/`encrypted`: folder. Relative paths are in the SSH user's home; on a disk it must be absolute (the disk's mount point and a folder) |
| `dataset` | `pool/path` | `format = zfs`: dataset that receives the snapshots, e.g. `tank/backup/myhome` |
| `keep` | int, default 14 | `zfs`/`encrypted`: backups kept at the destination |
| `priority` | int, default 100 | order: lower first, then by name. The backup runs in it, `neo restore` tries it (§4) |

Configs from before had these keys directly in `services.backup`; migration
`015-backup-destinations` moves them to `destinations.main`, and `neo restore` reads the old layout
the same way (a repo restored from an old backup).

### A run

1. `flock` on `/run/neo-backup.lock` (the nightly run waits for a key rotation).
2. One snapshot `zroot/neo@neo-backup-<UTC>` (Disko only): every destination gets the same moment.
3. Each destination in order, in a subshell with its own variables (a Nix-generated file per
   destination, listed in `NEO_DESTINATIONS`). A failure (`die`, or any failing command under
   `errexit`) ends that destination only; the run logs it and goes on. Log lines are tagged
   `backup [<name>]`; rsync/restic output goes to `/var/log/backup-<name>.log`.
4. Local snapshots are pruned (below), then the unit fails if any destination failed, so the web
   UI status shows it.

**ZFS bases.** An incremental `zfs send` needs a snapshot the destination already has, and with
several ZFS destinations those differ (one was unreachable last night). Each successful ZFS
destination writes the snapshot it received to `/var/lib/neo-backup/zfs-base-<name>`. Pruning keeps
the snapshots named there (also of disabled ZFS destinations; files of removed ones are deleted) and
destroys the other `neo-backup-*` snapshots. While an enabled ZFS destination has no file (its first
run failed, or the machine was just restored and `/var/lib` is new), the older snapshots all stay,
since one of them may be its base; only this run's snapshot goes if no one received it.

The combinations:

- **ssh + files**: `rsync -aH --numeric-ids --delete` to `user@host:remotePath/<basename sourceDir>/`
  (the layout of the old backup service, so existing backups keep going).
- **disk + files**: the same into a local path. Refuses to run when that path is on the root
  filesystem (an unplugged disk would otherwise fill `/`).
- **ssh + zfs**: `zfs send [-i <last common snapshot>] zroot/neo@neo-backup-… | ssh … zfs recv -u
  -F -o canmount=noauto -o com.sun:auto-snapshot=false <dataset>`. Needs root or
  `zfs allow <user> create,receive,mount,destroy,hold <parent>` on the destination. Locally only
  the bases of the ZFS destinations are kept (above); the destination keeps `keep`. The first run is a full send and refuses a destination dataset that exists already
  without a common snapshot, instead of overwriting it.
- **disk + zfs**: the same, `zfs recv` locally. The pool (first part of `dataset`) is imported
  with `-N` when needed and exported again afterwards.
- **ssh/disk + encrypted**: restic over SFTP (`sftp.command` = the backup's ssh with its key and
  options), or into the folder on the disk. See §3.

**SSH key.** The default is now the SSH **host key** (`/etc/ssh/ssh_host_ed25519_key`; the backup
runs as root). The host key is escrowed in `identity.sops.yaml` and restored on reinstall, so a
restored machine logs in to its backup with the same key, and `neo restore` can read the backup with
it. The old default (`homeserver`'s own key) was not in DATA and was lost with the machine.
Migration `013_backup_keeps_ssh_key` pins the old default in existing configs that enabled backup.

## 3. Encrypted format

Goal: the destination (a friend's server, a hoster, a disk that gets lost) only ever holds
ciphertext, and the backup still opens **without the config repo**: the same admin SSH key that
opens `secrets.sops.yaml` opens the backup, and the config repo is inside it.

### Two layers

1. **Data key.** A random 32-byte password of a restic repository. restic encrypts and
   authenticates everything (AES-256-CTR + Poly1305-AES), deduplicates and keeps versions.
2. **Header `neo-backup.age`.** The data key, encrypted with [age](https://age-encryption.org) to
   the same recipients as `secrets.sops.yaml`: the SSH host key (`core.secrets.identityFile`)
   and every ssh-ed25519 key in `core.ssh.authorizedKeys`. Plain text inside:

   ```
   version=1
   format=restic
   repository=restic-20261103-021500
   password=<base64 of 32 random bytes>
   ```

At the destination, in the backup folder (`remotePath`):

```
neo-backup.age        header
restic-<UTC>/         the repository it names
```

There is no key in the settings and none to escrow: the host key opens the header on every run,
the admins open it on restore. The data key is not stored on the server between runs; each run
decrypts the header into its private `RuntimeDirectory`.

### A run (`backup.sh` `backup_encrypted`)

1. Fetch the header (sftp `get`, or a plain copy on a disk) and decrypt it with the host key.
   A header the host key can't open is refused ("belongs to another server or an old key"), it is
   never overwritten.
2. No header: new data key, new repository `restic-<UTC>`, `restic init`.
3. `restic backup .` from the ZFS snapshot folder (or the live folder without ZFS), host
   `core.hostname`, tag `neo`, `--group-by host,tags` (the snapshot path changes every run, so
   the parent is found by tag). Paths in the snapshot are relative to `/var/neo`.
   `excludedDirs` as with rsync: a path is anchored at the folder, a plain name matches anywhere.
   Exit code 3 (some files unreadable) is logged, not fatal.
4. Write the header again, encrypted to the **current** recipients, and upload it as
   `neo-backup.age.tmp` + rename (sftp posix-rename; plain `rm` + rename as a fallback). The
   header goes up only after a snapshot exists, so a header always opens a backup. Adding or
   removing a key in System → SSH applies at the next run.
5. `restic forget --tag neo --keep-last <keep> --prune`.
6. Every other `restic-*` folder in the backup folder is removed: the old repository after a
   rotation, and repositories of first runs that failed before their header went up. Over SSH
   this is `rm -rf` through the shell; without shell access (restricted accounts) it is logged
   to delete by hand.

### Key rotation (`neo backup rotate-key`)

Starts `backup-rotate-key.service` (the same script, `NEO_ROTATE=true`; only defined when a
destination is encrypted, and kept out of `systemdUnits`, so the service target never starts it).
For every encrypted destination (the others are skipped): new data key, new repository, **full**
backup into it, new header, then the old repository is deleted. Each destination has its own data
key. Until the new header is up the old backup stays complete and usable; an interrupted
rotation leaves a stray `restic-*` folder that the next successful run removes.

Rotation replaces the repository instead of `restic key passwd` on purpose: a restic password
only wraps the repository's master key, so someone who once had the data key and a copy of the
repository's `keys/` would keep access. A new repository has a new master key. The cost is one
full upload.

When to rotate: after removing someone's key from System → SSH (they stop being a recipient at
the next run, but may know the data key), or when the server's host key may have leaked.

### Security of the header

`neo-backup.age` may be public. age encrypts a random 128-bit file key to each recipient with
X25519 (ssh-ed25519 keys are converted to X25519), the body with ChaCha20-Poly1305. Without one
of the private keys there is nothing to guess: no password, no low-entropy secret, and the data
key inside is 256 random bits. What it reveals: the number of recipients and, per recipient, a
4-byte tag of the SSH public key (lets someone holding a candidate public key, e.g. from GitHub,
check that it is one of the recipients). Limits: anyone with an admin key or the host key opens
it; X25519 is not post-quantum, so a recorded backup could be opened by a future large quantum
computer (true for SSH and sops today as well). Passphrase-protected admin keys work (age asks).

### Restoring without the config repo

`neo restore --from-backup <backup>` (no repo argument), or only the repo:
`neo backup extract-config <backup> [--out dir]`. `<backup>` is `user@host:folder` (relative to
the home folder), `ssh://user@host:port/abs/folder`, or a local folder (a mounted disk).

1. **Detect** what is there (sftp `ls`): `neo-backup.age` → encrypted; `neo/DATA` (or `DATA`) →
   plain files. ZFS backups can't be read without the repo.
2. **Open** the header with `--key` (default `~/.ssh/id_ed25519`), or the host key once known.
3. **Extract** `DATA/AppData/configuration` from the newest snapshot (`restic restore --include`);
   if `configPath` was elsewhere, the shortest folder with `settings.toml` next to `.git`.
   Plain files: rsync of the same folder.
4. Uncommitted changes in that repo (settings saved in the web UI) are committed ("restore:
   uncommitted changes from the backup") so the clone carries them; its own `origin` stays the
   remote of the restored repo.
5. The normal restore continues with that repo. `--from-backup` also replaces the settings'
   backup location as the data source. With `--host`, the client extracts the repo (it has your
   key) and passes `--from-backup` on; a disk path then makes no sense and is refused.

Preconditions: your key was in System → SSH at a backup run after it was added; you can log in
to the backup computer yourself (the server logs in with its host key, which is escrowed in the
repo you don't have). Add your own public key there too, or use the hoster's other access.

### Not yet: ZFS raw sends

The ZFS format could get the same property with `zfs send -w` (raw, still encrypted) and the
header as a user property of the destination dataset (`neo:key`, fits the 8 KB limit for a few
recipients). It needs `zroot/neo` encrypted **on the server**, which `encrypted` files do not:

- Disko creates `neo` with `encryption=aes-256-gcm keyformat=raw keylocation=file:///var/lib/neo/backup/zfs.key`
  (on the unencrypted root: protects the backup, not the local disk), generated in a
  `preCreateHook`; the key must be loaded before `/var/neo` is mounted in the initrd
  (`sysroot.mount` first, then `zfs load-key`), and `zfs-restore.sh` must load it too.
- Existing machines can't encrypt a dataset in place: `zfs send | zfs recv -o encryption=on` into
  a new dataset with the services stopped, needing room for a second copy, then a full send to a
  new destination dataset.
- Restore: raw receive into the new `zroot/neo`, data key from the header (same recipients),
  written to the key file, `zfs load-key`. Rotation is `zfs change-key` for the wrapping key; a
  new master key again needs a new dataset and a full send.
- Boot changes need `just test-disko-boot`.

## 4. `neo restore`

```
neo restore <repo> [--host root@IP] [--disk /dev/disk/by-id/…] [--key ~/.ssh/admin]
                   [--data backup|none] [--from-backup <backup>] [--keep-hardware] [--yes] [--reboot]
neo restore --from-backup <backup> …          # the config repo from the backup (§3)
```

**Several destinations.** Without `--from-backup`, the preflight checks the destinations in
`priority` order and takes the first one it can read; the unreadable ones are printed. The plan
names it and how many follow. If copying from it fails, the data folder is emptied (the data
dataset mounted again after a failed `zfs recv`) and the next readable destination is tried. Only
when none works does the restore stop. `--from-backup` names exactly one backup, without fallback.

Two modes, one code path:

- **On the new machine** (no `--host`): booted from any NixOS installer, as root. Everything below
  runs locally.
- **From a client** (`--host root@IP`): a laptop with Nix. If the target is not a NixOS installer
  yet (any Linux with root SSH, e.g. the hoster's Ubuntu), **nixos-anywhere** kexecs it into one.
  The repo, the admin key and every locked flake input (`nix flake archive`, fetched with the
  client's credentials) are copied into the installer's RAM, and the local mode runs there as a
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
6. **Data** from the backup (the first destination that works, see above) into the new `zroot/neo`: rsync of `<remotePath>/<basename>/` for
   `files`, `restic restore latest` for `encrypted` (header opened with the escrowed host key or
   the admin key; the next backup continues in the same repository), `zfs send` of the newest
   backup snapshot into `zroot/neo` for `zfs` (the next backup from the new machine continues
   incrementally). The backup's copy of the config repo is moved
   aside (`configuration.from-backup`); the repo from git is installed.
7. **Install**: repo to `<configPath>`, host key to `/mnt/etc/ssh`, `nixos-install`.
8. **Check** the firmware boot entry (UEFI) and, with `--reboot`, unmount, export and reboot.

Native NixOS service users (Hermes) get their uid from the new machine; their activation re-owns
their appdata when the folder still belongs to the old uid, so the copy does not need fixing by hand.

## 5. Not covered

- Encryption of the ZFS format (§3, "Not yet"); use `encrypted`, or an encrypted remote dataset.
- `git+file:` plugins that live in DATA: restore refuses and names them; put them into git.
- Restores of single services from the remote backup (local ZFS snapshots cover that).

## 6. Backup store

The **backup-store** service (`nix/services/backup-store`) keeps other servers' backups, and is
reachable only over HTTPS. On the client it is a destination with `target = "store"`.

### SSH inside HTTPS

```
backup.sh / neo restore → ssh ProxyCommand: websocat → https://<sub>.<domain> → SWAG
  → backup-store-tunnel (websocat, unprivileged) → backup-store-sshd (127.0.0.1) → neo-store-gate
```

Why not one HTTP server per format: restic's rest-server, WebDAV and a streaming endpoint would be
three servers plus new backup and restore code for each, and WebDAV loses owners and hardlinks.
With a tunnel the three formats keep the code paths of §2 and §3, in both directions: `backup.sh`
sees `NEO_TARGET=ssh` with a different ssh command line, and `neo restore` a `Location::Ssh` with a
`ProxyCommand`. The cost is double encryption (TLS around SSH).

There is no tinyauth (`auth.available = false`) and no token: the SSH key is the login, and a token
would be one more thing a restore without the config repo doesn't have. All connections reach sshd
from loopback, so `PerSourcePenalties` is off and addresses in its log say nothing.

The client's first contact is authenticated by the store's TLS certificate, then the SSH host key
(the store's own `core.secrets.identityFile`, so it survives a reinstall) is pinned in
`/var/lib/neo-backup/known_hosts`. A store that moves to another host key needs that line removed.

### Accounts and confinement

`clients.<name> = { format; publicKeys; }` makes one account `backup-<name>`, home
`<path>/<name>` (0700). Every key is `restrict,command="neo-store-gate <format> <dataset>"`.

- **`neo-store-gate`** (`cli/src/bin/neo-store-gate.rs`) splits `SSH_ORIGINAL_COMMAND` without a
  shell and only runs what `backup.sh` and `neo restore` send: per format, an allowlist of commands,
  flags and paths inside the home or the account's datasets.
- **The sshd unit's mount namespace** is the second wall, inherited by sessions:
  `ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`, the store the only writable path, the Neo data
  volume and `/var/lib` inaccessible. That covers what an allowlist can't: symlinks made over SFTP and
  arguments of allowed programs. World-readable system files (`/etc`, `/nix/store`) stay readable
  over SFTP; they hold no secrets. The store's root is 0755 (rsync opens every parent folder), so a
  client can list the other clients' names, not their content.

| Format | Allowed | Notes |
|--------|---------|-------|
| `files` | `rsync --server`, sftp, `chmod u+w`, `test -d` | the gate adds `--fake-super`: owners, modes and devices are stored in `user.rsync.%stat` xattrs (the account isn't root), and a restore through rsync brings them back. The folder itself shows the account as owner, and symlinks as small files |
| `encrypted` | sftp, `rm -rf …/restic-*` | |
| `zfs` | `zfs list/recv/create/destroy/send/get` in fixed forms | `zfs allow` on `<dataset>/<client>`; `recv` only below it with fixed `-o`, always `-u`; `destroy` only `@neo-backup-*` snapshots |

A ZFS client's `dataset` setting is `<store dataset>/<client>/neo`, the path the gate checks.

### Storage

`storage = "dataset"` (every format) or `"folder"` (`files`/`encrypted`, any filesystem). Both are
outside `volumes.root` by assertion, also `sourceDir`, so this server's own backup never includes
them, and `zroot/neo` snapshots don't either. A dataset on the main pool or a folder on the system
disk works with a build warning: `neo restore` of this server erases the disk. A declared mount for
the folder is required (`RequiresMountsFor`), so an unplugged disk fails the unit instead of filling
`/`. A pool other than Disko's is imported through `boot.zfs.extraPools`.

**Encryption at rest** (`encrypted`, dataset only): `encryption=aes-256-gcm`, `keyformat=passphrase`,
key from `encryptionKey` (a secret, rendered to a root-only file). `backup-store-setup` loads it at
boot, and on every run with the key loaded `zfs change-key` re-wraps the dataset to the current
value, so changing the setting is safe. Client datasets and received streams inherit the encryption.
This protects a stolen disk, not against the store: non-raw `zfs send` and `files` are readable while
the key is loaded. Clients that don't trust the store use `format = "encrypted"`, or wait for §3
"Not yet". ZFS can't encrypt an existing dataset, so the setup refuses instead of carrying on without.

The key only exists after activation, so nothing may try to load it at pool import: NixOS's default
`boot.zfs.requestEncryptionCredentials = true` fails the import of the whole pool and the machine
stops in emergency mode, also with the store disabled while the dataset still exists. Disko sets it
to false; without Disko the store's assertion asks for it.

Removing a client removes its account and keeps its data.

### Restore

The store is a normal source: from the settings (a destination with `target = "store"`), or
`neo restore --from-backup https://<client>@<store>/<folder>` / `neo backup extract-config` the same
way (`user` is derived as `backup-<client>`). The admin key must be in the client's `publicKeys`.
`websocat` is in the `neo` package's restore tools. A store itself has no backup of what it holds.
