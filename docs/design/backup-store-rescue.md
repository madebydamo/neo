# Backup store: rescue portal

Status: built, except the metrics and alert of §2 (see §6). Extends
[backup-restore.md](backup-restore.md) §6.

Today `https://<subdomain>.<domain>` of the backup store answers a browser with websocat's
"only WebSocket connections" text. It becomes the **rescue portal**: the page to open when a
server is lost. The backup clients keep using the same address for the tunnel.

The portal shows:

1. **Overview**: every server that backs up here, and whether its backups are current.
2. **Three ways to restore** each server, with the requirements of each way listed and checked
   where the store can check them:
   - **From this server**: give the new machine's address and your admin key, the store does
     the rest.
   - **From your computer**: a computer with Nix runs `neo restore --host …`.
   - **From a NixOS machine**: the new machine is booted into the NixOS installer (or any NixOS)
     and restores itself.

The store keeps its current property: it holds no key to its clients' data. Your admin key comes
in for one rescue job and is deleted after it.

## 1. One address, two uses

SWAG decides per request (`backup-store/swag.nix`):

```
https://backup.<domain>/
  Upgrade: websocket  → backup-store-tunnel (websocat → sshd), no login, as today
  anything else       → login (tinyauth) → Neo web UI, portal routes only
```

nginx:

```nginx
location / {
  if ($http_upgrade = "websocket") { rewrite ^ /.neo-tunnel last; }
  # auth (tinyauth) as for every app with login
  proxy_set_header X-Neo-Site backup-store;
  proxy_pass http://host.docker.internal:<neo web port>;
}
location = /.neo-tunnel {
  internal;
  proxy_pass http://host.docker.internal:<tunnelPort>/;
}
```

`rewrite … last` inside `if` is one of the safe uses of `if` in nginx. The tunnel location is
`internal`, so a browser cannot open it by path. The clients connect to `/` as now: no client
change.

- **Login.** `auth.available` becomes true. `auth.enabled` follows the default login like other
  apps; it applies to the portal only. The tunnel never has tinyauth: the SSH key is its login.
  An operator can turn login off for the portal; the portal then shows only the generic help
  (§3), no client names, keys or sizes.
- **Neo web, portal mode.** A request with `X-Neo-Site: backup-store` gets the portal at `/`
  and only the routes the portal needs (its pages, `/static`, the rescue routes). Every other
  route answers 404 on this site. A request guard in Rocket does it, not a path list per route.
  SWAG sets the header; neo web listens only on the `neo-host` bridge, so nothing else can set it.
- **Who may rescue.** On the store's address, the client list and the rescue need the portal's
  login; without it the page shows only the general text. Tinyauth decides who may log in, as for
  every app. In the normal web UI the portal is at `/backup-store/portal` with the web UI's login.

## 2. What the store knows

The store sees each client's home folder (`<path>/<client>`) or dataset. It can read only what
is not encrypted by the client.

| Format | Last backup | Versions | Size | Config repo readable by the store |
|--------|-------------|----------|------|-----------------------------------|
| `files` | marker (below), else newest mtime of the top folders | 1 | `du` | yes (plain files) |
| `encrypted` | marker, else newest file in `restic-*/snapshots/` | files in `restic-*/snapshots/` | `du` | no: needs a recipient key of `neo-backup.age` |
| `zfs` | newest snapshot `@neo-backup-<UTC>` | number of those snapshots | `zfs list -o used` | yes (read-only mount of a snapshot) |

Also from the store's own data:

- **Last connection**: `Accepted publickey for backup-<client>` in the journal of
  `backup-store-sshd`. A connection without a new backup means the run failed on the client.
- **Keys**: comment, type and SHA256 fingerprint of each of the client's `publicKeys`; for an
  encrypted client also the admin keys of its `neo-backup.recipients`, each with its role
  (`server`, `unlock`, or `access` = can connect but cannot open the backup). See
  [backup-restore.md](backup-restore.md), Restore keys.
- **Backup folders**: in the home, each folder that holds `neo-backup.age` (encrypted) or
  `neo/DATA` / `DATA` (files). This is the `<folder>` of a `--from-backup` URL. One client can
  have more than one (two destinations to the same store).
- **Reachable from the internet**: the store's own `ingress` includes `web`.

### Report unit

`backup-store-report.service` (root, timer every 15 minutes and after `backup-store-setup`)
writes the facts above to `/var/lib/neo-backup-store/report.json`. The portal reads only that
file: the client homes are 0700 per account, and `du` on a large folder is too slow for a page
request.

Not built yet: metrics `neo_backup_store_last_backup_timestamp_seconds{client}` and
`neo_backup_store_size_bytes{client}`, and an alert when a client has no new backup for 36 hours.
The agent's textfile folder is writable now (`neo.monitoring.textfileDir`, see
[monitoring.md](monitoring.md)). The client already alerts on its own failed runs and on the
time since its last success per destination (`neo_backup_*`).

### Client change: a backup marker

At the end of each successful run to a store (`files` and `encrypted`), `backup.sh` uploads
`neo-backup.last` to the backup folder, in plain text:

```
version=1
time=2026-11-19T02:15:03Z
host=brugg
neo=<neo revision>
```

It holds no secret. It gives the store an exact "last good backup". Uploaded with sftp `put` +
rename, like the header; the gate allows sftp for both formats already. ZFS needs no marker: the
snapshot name is the time. Clients without the change fall back to the mtimes above; the portal
marks those times "estimated".

## 3. The portal

### Start page

```
Backup store · backup.example.com

If a server is lost: pick it below and choose how to restore it.

┌ brugg ───────────────────────────────── ● Backed up 5 hours ago ┐
│ Encrypted · 14 versions · 182 GB · last connection 5 hours ago   │
│ Keys: brugg host key · damian@laptop · admin@desktop             │
│ [ Restore brugg ]                                                │
└──────────────────────────────────────────────────────────────────┘
┌ wohlen ──────────────────────────────── ● Backed up 3 days ago ─┐
│ …                                                                │
```

State colours: green (backup within 36 hours), orange (older), red (none for 7 days, or never).
A client in the settings without data shows "No backup yet". A home folder of a removed client
shows "Removed: keeps <size>" without actions.

Below the list, a short section "About this store": what it is, that it only holds what the
servers send, and that it has no backup of its own data. Its part "If this address is down"
names `neo restore --from-store <your SSH login to this server>` and how to open the portal
through that login: `ssh -N -L 8091:127.0.0.1:8091 <login>`, then
`http://127.0.0.1:8091/backup-store/portal`. The client page repeats that line.

### Restore page of a client

`Restore brugg` opens a page with four tabs, one per way. The first is **Over SSH to this
server** ([backup-store-ssh.md](backup-store-ssh.md) §3), then the three ways below. Each tab has the same parts:
**You need** (a checklist), **Steps**, and the **command** with every value filled in (store
address, client name, folder, format). A check mark means the store verified it; a hollow
circle means you must check it yourself.

**Common to all three ways**

- ○ A new machine. Any Linux with root SSH (a VPS from a hoster works), or one booted into the
  NixOS installer. Its disk will be erased.
- ○ The old server is off. Two servers with the same key would back up into the same place.
- ✓ / ✗ The new machine can reach this store: `ingress` includes `web` (✓), or it must be in the
  same LAN / tailnet (✗ with that text).
- ○ An admin key: the private half of one of these keys: `damian@laptop (SHA256:…)`,
  `admin@desktop (SHA256:…)`. The server's own host key does not count: it is lost with it.
  `encrypted`: the key was also in System → SSH on brugg at a backup run after it was added.
- After the restore: point brugg's DNS names (and the streamproxy edge, if any) to the new IP.

**0. Over SSH to this server** (works when the store's HTTPS address is down)

You need, in addition: Nix with flakes, your normal SSH login to this server from your computer,
and root SSH to the new machine with your admin key.

```
nix run github:madebydamo/neo -- restore \
  --from-store <your login to this server> --client brugg \
  --host root@<new IP>
```

`zfs`: with the config repo in front, as in way 2. Below the command: this server's names (its
hostname, its global addresses without Docker bridges, its Tailscale name), so the reader can
build the login (`homeserver@<name>`). When `backup-store-tunnel` is not active, the page shows
an alert "The web address of this store is down" and this tab is the one open.

**1. From this server** (the form only with the portal's login)

You need, in addition:
- ○ Root SSH to the new machine with your admin key (most hosters ask for a public key when you
  create the machine). The store logs in to it with that key, not with a key of its own.

A form: new machine `root@IP[:port]`, admin key (paste or file, passphrase field), then
**Check** and **Restore** (§4). The new machine reads the backup over the store's HTTPS address;
while the tunnel is down, the tab says so and points at way 0.

**2. From your computer**

You need, in addition:
- ○ Nix with flakes on your computer (Linux, macOS or WSL). Install line:
  `sh <(curl -L https://nixos.org/nix/install) --daemon`, then
  `experimental-features = nix-command flakes` in `nix.conf`.
- ○ Root SSH from your computer to the new machine with your admin key.

```
nix run github:madebydamo/neo -- restore \
  --from-backup https://brugg@backup.example.com/backups \
  --host root@<new IP> --key ~/.ssh/id_ed25519
```

`zfs`: `--from-backup` cannot read a ZFS backup yet (§6), so the command takes the config repo:
`neo restore <repo URL> --host root@<new IP>`, and the tab says where that repo is
(`core.git` remote if the store can read it from the snapshot, else "your config repo").

**3. From a NixOS machine** (the new machine restores itself)

You need, in addition:
- ○ The new machine booted into the NixOS installer (link to the ISO) or running NixOS, with
  internet access, as root.
- ○ Your admin key on that machine, e.g. `/root/admin_key` (copy it with `scp`, or paste it in
  the console). An SSH agent is not enough: age needs the key file.

```
nix --extra-experimental-features 'nix-command flakes' run github:madebydamo/neo -- restore \
  --from-backup https://brugg@backup.example.com/backups --key /root/admin_key
```

Then: delete `/root/admin_key` if the restore did not (`--shred-key`).

The text of the tabs comes from one template. The portal fills it with live values; the
`neo-backup-store` Hermes skill gets the same text with placeholders, so the assistant can guide
someone through it. The assistant never sees a key.

## 4. Restore from this server

### Flow

1. **Check** runs `neo backup-store rescue brugg --host … --check` and shows its plan: disks
   found on the new machine, data size, hostname, domain. A disk choice appears when there is
   more than one disk (`--disk`).
2. **Restore** asks to type the client name (in-page dialog, never `window.confirm`), then
   starts. Progress shows in the operation monitor (new `OperationKind::Rescue`; steps Check,
   Boot installer, Copy, Erase disk, Data, Install, Finished), fed from the job's journal.

The new machine reads the backup over the store's HTTPS address (`--from-backup https://…`). The
job itself never uses the tunnel: it takes the config repo from the local copy. So when the HTTPS
address is down, this way does not work; the page says so and points at the SSH way (§3).

### The job

A CLI command does the work; the portal route only starts it:

```
neo backup-store rescue <client> --host root@IP [--disk …] --key <file> [--folder <f>] [--check]
```

It runs as root in a transient unit `neo-rescue-<client>` (`systemd-run`), so it survives a
restart of the web UI. Steps:

1. **Config repo** from the local data, no tunnel:
   - `files`: copy `DATA/AppData/configuration` from the folder.
   - `encrypted`: open `neo-backup.age` with the admin key, `restic restore` of the config folder
     from the local repository (the code of `extract-config`, on a local path).
   - `zfs`: mount the newest `@neo-backup-*` snapshot read-only in a private folder and copy
     the config folder.
2. **Short-lived backup login**: a new ed25519 key pair in the job's runtime folder. Its public
   key goes into `/run/neo-backup-store/rescue/backup-<client>` with
   `restrict,command="neo-store-gate --read-only <format> <dataset>"`. sshd reads that folder as
   a second `AuthorizedKeysFile`. The job removes the file at the end; `/run` is empty after a
   reboot anyway.
3. **Restore**: `neo restore <repo> --host root@IP --key <admin key> --backup-key <short-lived
   key> --from-backup https://<client>@<store>/<folder> --shred-key --yes --reboot`. The new
   machine logs in to the store with the short-lived key over the public tunnel. The admin key
   opens `identity.sops.yaml` (and the header, for `encrypted`), as in a restore from a laptop.
4. **Clean up**: remove the authorized key, shred the key files, unmount.

### Keys

- The admin key exists only in the job unit's `RuntimeDirectory` (tmpfs, 0700, root). It is
  never written to the ops store, the journal, a lock note or the settings. The POST that
  carries it is not logged and never enters a template context, so `web/redact.rs` never sees it.
- A passphrase-protected key is decrypted with `ssh-keygen -p` into the same folder; age and
  sops in the job cannot ask.
- The assistant cannot start a rescue: the route refuses agent sessions, and the key fields are
  not settings, so no proposal or `generate` path reaches them.
- The short-lived key only reads: the gate's `--read-only` mode allows the commands `neo restore`
  sends and nothing that writes or deletes.

### Locks

Service scope `backup-store-rescue-<client>`, exclusive, for the job: two rescues of the same
client cannot run at once. The system scope stays free, so the store can still activate during a
restore of an hour or more; sshd keeps running sessions (`KillMode=process`). Other clients keep
backing up.

## 5. Changes by area

| Area | Change |
|------|--------|
| `nix/services/backup-store/swag.nix` | WebSocket → tunnel (internal location), rest → neo web with `X-Neo-Site`, tinyauth for the rest |
| `nix/services/backup-store/option.nix` | `auth.available = true` (portal only); description and notes mention the portal |
| `nix/services/backup-store/default.nix` | `backup-store-report` service + timer (in `systemdUnits`); second `AuthorizedKeysFile`; tmpfiles rule for `/run/neo-backup-store/rescue`; `/etc/neo/backup-store/store.json` for the CLI |
| `nix/services/backup-store/monitoring.nix` | follow-up (§6): alert "no backup for 36 hours" |
| `nix/services/backup-store/skills.nix` | restore text with placeholders |
| `cli/src/bin/neo-store-gate.rs` | `--read-only`: `files` → `rsync --server --sender`, sftp `-R`, `test -d`; `encrypted` → sftp `-R`; `zfs` → `list`, `send`, `get`. Tests per format |
| `nix/services/backup/backup.sh` | upload `neo-backup.last` after a successful run to a store |
| `cli/src/commands/backup_store/` | new: `neo backup-store report` (used by the unit) and `rescue` |
| `cli/src/commands/web/backup_store.rs` | `PortalSite` fairing for `X-Neo-Site`, portal routes (start page, client page, rescue start), key handoff through `/run/neo-web` (neo-web `RuntimeDirectory`); `OperationKind::Rescue` |
| `cli/templates/backup_store/` | start page and client page (three CSS-only tabs); `static/backup_store_portal.js` reads a key file into the form |
| backup-store service texts | `notes` name the portal address |
| `docs/RESTORE.md`, `backup-restore.md` §6 | link here; the portal as a starting point |
| `agent/skills/neo-core` | gate rule now also names read-only mode |

## 6. Decisions and follow-ups

1. **Neo web off**: not handled. The Neo web UI is always on where a store runs.
2. **Private plugin inputs** (way 1): no change needed. `neo restore` on the new machine unlocks
   `core.git.tokens` with the admin key and fetches what the store could not copy.
3. **The store itself is lost**: accepted. Every server keeps at least two backups, so one store
   can be lost; the other place (or a laptop with the repo) restores it.
4. **Old server still running**: a warning on the client page and in the job's log when the client
   connected in the last hour. No refusal.
5. **ZFS from a laptop**: stays as it is. `encrypted` and `files` hand out the repo with
   `--from-backup`; a ZFS stream cannot be read in part over the tunnel, so ways 2 and 3 take the
   repo (the page shows its remote when the store can read it). Way 1 reads the snapshot locally.
6. **Follow-up**: metrics and the "no backup for 36 hours" alert (§2).
