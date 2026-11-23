# Backup store: restore over the server's normal SSH

Status: built (2026-11-23); differences from this design in "As built" at the end. Extends [backup-restore.md](backup-restore.md) §4 and §6 and
[backup-store-rescue.md](backup-store-rescue.md).

## 1. Problem

A backup store is reachable only over HTTPS: `wss://backup.<domain>` → SWAG →
`backup-store-tunnel` → `backup-store-sshd` on `127.0.0.1:18023` → `neo-store-gate`.

If the store's HTTPS address goes through a streamproxy edge, and the edge is the machine that is
lost, its backup sits behind its own front door. Example: meilen (edge) backs up to brugg's store
at `https://backup.dev1.syntelis.ch`, which goes through meilen.

## 2. Constraint and decision

**Constraint.** The operator knows how to reach each server with SSH (`ssh homeserver@brugg`,
an ssh_config alias, Tailscale, LAN, a jump host). Neo does not care how. Neo starts from that
login.

**Decision.** The second way in is the operator's **normal admin login** to the store server.
From there, plain OpenSSH reaches the store's sshd on loopback:

```
your computer ── ssh homeserver@brugg (your route, your ssh_config) ──┐
                                                                     │ ProxyJump (direct-tcpip)
                                         backup-store-sshd 127.0.0.1:18023 ─▶ neo-store-gate
```

- **No new port, no new listener, no firewall change.** NixOS's sshd allows TCP forwarding by
  default, so `ssh -J homeserver@brugg -p 18023 backup-meilen@127.0.0.1` works today.
- **The gate stays the wall.** The inner hop logs in to `backup-<client>` with a store key, as
  over HTTPS. The admin login only carries the bytes. The rescue and recipient keys (read-only)
  work the same way.
- **Backups do not change.** Clients keep backing up over HTTPS with their host key. A client's
  host key gets no login on the store server's port 22. This plan is about restoring.

On top of the plumbing, the CLI and the store **find and explain** the backups (§4): "this store
holds meilen (encrypted, 14 versions, last backup 5 hours ago) and your key can open it".

## 3. Store side

### `neo backup-store list` (new, runs as the admin user)

Reads `store.json` and `report.json` and prints what the store holds, without root:

```
$ ssh homeserver@brugg neo backup-store list
Backup store on brugg (https://backup.dev1.syntelis.ch)
Store SSH: 127.0.0.1:18023, host key ssh-ed25519 AAAA… (SHA256:…)

meilen   encrypted  14 versions  182 GB  last backup 2026-11-22 02:15 (5 h ago)
         folders: backup/meilen
         can restore: damian.moser@syntelis.ch (SHA256:…), alexis… (SHA256:…)
gossau   zfs        30 versions   40 GB  last backup …
         can restore: gossau host key only — add an admin key to its publicKeys
```

`--json` is the same for the CLI (§4). It holds no secret: names, sizes, times, public keys,
fingerprints, the store's public host key (`core.secrets.identityFile` + `.pub`).

Needed for this:

- `report.json` readable by the admin user (`homeserver` group, 0640). It holds no secret.
- `store.json` gets `sshPort` and `hostKey` (the public line). The host key also lets the CLI
  pin the inner hop without trust on first use: the outer hop is already authenticated by the
  operator's `known_hosts`, so a key read through it is trusted (§4.3).

### Portal

The rescue portal (`backup-store-rescue.md` §3) shows the SSH way as the first option on every
client's restore page, with the values filled in:

```
From your computer, over SSH to this server:
  neo restore --from-store <your login to this server> --client meilen --host root@<new machine>
  Your login to this server is how you normally reach it, for example homeserver@brugg.
  This server's names: brugg, 192.168.1.20, brugg.tail1234.ts.net (from its interfaces and Tailscale)
```

The portal itself is also reachable without the HTTPS address: `ssh -N -L 8091:127.0.0.1:8091
homeserver@brugg`, then `http://127.0.0.1:8091/backup-store/portal`. The portal says so in its
"If this address is down" box.

### Texts

- `enabled`: "Backup store: other servers keep their backups here" (drop "reachable only over
  HTTPS").
- `notes`: "If the web address is down (for example when it runs through the server you need to
  restore): `neo restore --from-store <your SSH login to this server>`. Your normal SSH login is
  the way in; the store's own keys still decide what you can read."

## 4. Client side: `neo restore --from-store`

### 4.1 Commands

```
neo restore --from-store homeserver@brugg                       # list, pick, restore
neo restore --from-store homeserver@brugg --client meilen --host root@203.0.113.7
neo restore <repo> --from-store homeserver@brugg --host …       # repo given: store destinations use this login
neo backup list --from-store homeserver@brugg                   # only look
neo backup extract-config --from-store homeserver@brugg --client meilen <dir>
```

`<login>` is anything `ssh` takes: `user@host`, `ssh://user@host:port`, an ssh_config alias.
Neo passes it to the system `ssh` as it is, so the operator's `~/.ssh/config` (ProxyJump,
ports, keys, Tailscale names) applies.

### 4.2 What the CLI does, step by step

1. **Connect**: `ssh <login> neo backup-store list --json`. Errors are explained:
   - SSH fails: "could not log in to <login>: <ssh error>. Neo uses your normal SSH; check that
     `ssh <login>` works."
   - `neo backup-store` missing: "<login> runs no Neo backup store (or a Neo older than …)."
2. **Show** the clients as above. For each one, compare the store's restore keys with **your**
   keys (`ssh-add -L`, `~/.ssh/*.pub`, `--key`) and say per client: "you can restore this with
   ~/.ssh/id_ed25519" or "none of your keys can open it; it accepts: …".
3. **Pick**: `--client`, or ask (only clients you can open are offered first). One backup
   folder: taken; several: ask.
4. **Get the config repo** from that backup (`extract-config`, as `--from-backup` does today),
   unless `<repo>` is given.
5. **Restore** with the source "store over SSH" (§4.3), then the normal restore steps.

With `<repo>` given: every destination with `target = "store"` whose `storeUrl` equals the
store's `url` in `store.json` gets the SSH source **in front of** its HTTPS source. Other
destinations stay as they are, and the fallback order of `backup-restore.md` §4 still applies.

### 4.3 The source: `Location::StoreSsh`

```
ssh -o ProxyJump=<login> -p <sshPort> \
    -o HostKeyAlias=<store https host> \
    -o UserKnownHostsFile=<tmp>/known_hosts -o StrictHostKeyChecking=yes \
    -i <admin key> backup-<client>@127.0.0.1
```

- `<tmp>/known_hosts` holds the `hostKey` line from step 1, under `HostKeyAlias`. The outer hop
  uses the operator's own `known_hosts`.
- rsync, restic (sftp) and `zfs recv` take this as their ssh command, the same as the
  `Location::Ssh` with ProxyCommand that HTTPS gives today. `source.rs` models it as a
  `Location::Ssh` with these options, so the readers in `restore.rs` do not change.
- `--from-backup ssh+store://<client>@<login>/<folder>` is the explicit form, for scripts.

### 4.4 Where the data flows with `--host`

`neo restore --host` runs the restore **on the new machine**. The new machine has none of the
operator's aliases, keys or `known_hosts`. Two ways, tried in this order:

| Way | Path of the data | Works when |
|-----|------------------|------------|
| **direct** | new machine → `<login>` → store 127.0.0.1:18023 | the new machine can reach the store server itself (public IP, same LAN, a jump host it can reach) |
| **relay** | new machine → your computer → `<login>` → store | `ssh <login>` works from your computer. **Always the last resort, and enough on its own.** |

`--store-via auto|direct|relay`, default `auto`: direct, and relay when direct fails. `direct`
and `relay` force one way (for tests, or when one is known to be wrong).

#### Direct

The new machine needs three things it does not have. The CLI brings them:

1. **The route.** `ssh -G <login>` on the operator's computer resolves the alias to `user`,
   `hostname`, `port` and `proxyjump` (each jump host resolved the same way). The new machine
   gets these resolved values, never the alias.
2. **The outer hop's host key.** The operator's `known_hosts` lines for each resolved host
   (`ssh-keygen -F`) go into the restore's temporary `known_hosts` on the new machine, with
   `StrictHostKeyChecking=yes`. A host the operator never logged in to fails the probe (then:
   relay).
3. **The outer hop's login.** The CLI's session to the new machine uses `ssh -A`, so the outer
   hop logs in with the operator's agent. Without an agent (`SSH_AUTH_SOCK` empty) it tries the
   admin key that `--host` mode already copies to the new machine.

The agent is only there while the operator's session is open, and the restore runs in its own
unit (`neo-restore`), which cannot see the session's agent. So the session opens **one master
connection** to the store on the new machine, and the unit only reuses it:

```
# in the operator's ssh -A session on the new machine
ssh -f -N -M -S /root/neo-restore/store.sock -o ControlPersist=yes \
    -J <resolved login> -p <sshPort> -o HostKeyAlias=<store host> \
    -o UserKnownHostsFile=/root/neo-restore/known_hosts -o StrictHostKeyChecking=yes \
    -i /root/neo-restore/admin-key backup-<client>@127.0.0.1
```

The source in the unit is the same command with `-S /root/neo-restore/store.sock` (and
`ControlMaster=no`). Every later connection (header, `extract-config`, restic, rsync, `zfs recv`)
runs inside the master, so the restore needs the agent only for that first login, and keeps
running when the operator's connection drops, as today.

The probe is the master itself plus `ssh -S … true` (the gate answers `true`). Timeout 20 s.
When it fails, the CLI prints why ("brugg did not answer from the new machine", "host key of
brugg not in your known_hosts") and goes to the relay.

Agent forwarding gives root on the new machine the use of the operator's agent while the
session is open. The new machine is the operator's own fresh installer, so that is acceptable;
the plan before "Restore with this plan?" says "uses your SSH agent on <new machine> to log in to
<login>". `--store-via relay` avoids it. The master closes when the restore ends, and
`/root/neo-restore` is removed as today.

#### Relay

```
new machine ──(-R 127.0.0.1:Q)── your computer ──(-L 127.0.0.1:P, your ssh_config)── <login> ── store 127.0.0.1:18023
```

1. The operator's computer opens `ssh -N -L 127.0.0.1:P:127.0.0.1:<sshPort> <login>` (its own
   ssh_config, agent and `known_hosts`: whatever works for `ssh <login>` works here).
2. On the SSH session to the new machine it adds `-R 127.0.0.1:Q:127.0.0.1:P`.
3. On the new machine the source is `backup-<client>@127.0.0.1 -p Q` with the same
   `HostKeyAlias` and `known_hosts`, and the admin key that `--host` mode already copies there.

Both ends are loopback; the store's gate and the host key check are the same as for direct.

Cost: the data passes through the operator's line, and **the operator's computer must stay
connected until the data copy is done**. The restore prints "keep this terminal open until the
data is copied (step 6)". A dropped relay fails step 6 with "the relay through your computer
stopped; run the same command again to continue" (restic and rsync continue where they
stopped).

### 4.5 Running on the new machine itself

`neo restore` run locally (on the NixOS installer, `local/`) uses `--from-store` the same way,
without the relay: `<login>` must work from that machine.

## 5. The rescue from the store itself

`neo backup-store rescue` (the portal's "From this server") reaches its own sshd over the HTTPS
tunnel today (`--from-backup https://…`). It changes to `127.0.0.1:<sshPort>` directly, so it
also works when the store's HTTPS address is down. The new machine still reads the backup over
HTTPS (§4 of the rescue design) — that needs the HTTPS address. When it is down, the rescue page
says so and points at the SSH way (§3), which works from your computer.

## 6. Restoring everything: the edge case

To bring meilen back with brugg's store:

1. `ssh homeserver@brugg` works (your route: LAN, Tailscale, a port forward — your choice).
2. `neo restore --from-store homeserver@brugg --client meilen --host root@<new meilen>`.
3. After the restore: meilen's streamproxy is up again, so brugg's HTTPS address works again.

The restore's plan check (before "Restore with this plan?") also names inputs that may sit behind
the lost machine: a plugin or flake input on a host that does not resolve, a Nix cache
served from that host. It says which ones and how to override them; it does
not try to route around them. A section "Restoring your edge" in [RESTORE.md](../RESTORE.md) lists
this.

## 7. Phases

1. **Store**: `neo backup-store list [--json]`, `report.json` readable, `store.json` with
   `sshPort` and `hostKey`, texts.
2. **CLI**: `--from-store` for `neo restore` (local mode), `neo backup list`, `extract-config`;
   key matching; `ssh+store://`; with `<repo>`, store destinations get the SSH source first.
3. **`--host`**: direct with agent forwarding and the master connection, relay as fallback, `--store-via` (§4.4).
4. **Portal and rescue**: SSH way on the restore page, the rescue over loopback, RESTORE.md
   "Restoring your edge".

Store and client change together (AGENTS.md), but an older store fails with the clear message of
§4.2 step 1, so phase 1 can ship first.

No gate change: the inner hop sends the same commands as over HTTPS.

## 8. Tests

- **Rust**: parsing of `--from-store` logins and `ssh+store://`; the ssh options of
  `Location::StoreSsh` (ProxyJump, HostKeyAlias, StrictHostKeyChecking=yes); matching destinations
  to `store.json`; key matching against recipients and `publicKeys`; `list --json` from a sample
  `report.json`; the resolved route from sample `ssh -G` output (alias, port, nested ProxyJump);
  the order direct → relay in `auto`.
- **VM** (`just launch`, the VM backs up to its own store):
  - `neo backup list --from-store ssh://homeserver@localhost:2222` from the host lists the VM.
  - stop `backup-store-tunnel` (HTTPS way dead), then `neo backup check` and `extract-config` with
    `--from-store`.
  - a wrong `hostKey` in `store.json`: the inner hop refuses.
  - the admin user with a key that is no restore key: the inner hop is refused by the store sshd,
    and the CLI said so before trying.
- **Real**: brugg's store, `backup-store-tunnel` stopped for the test, into a scratch VPS
  (`neo restore --from-store homeserver@brugg --client meilen --host root@<scratch VPS>`), then
  shut the VPS down:
  - `--store-via direct` over a route the VPS can reach (brugg's Tailscale name with the VPS in
    the tailnet, or a test port forward); drop the operator's connection during step 6: the
    restore goes on.
  - `--store-via auto` with a route only the operator's computer has: the probe fails with its
    reason, the relay takes over and finishes.
  - `--store-via relay` without an SSH agent on the operator's computer.

## 9. Decided

1. **Data path with `--host`**: direct first (agent forwarding, resolved route, master
   connection), relay through the operator's computer when direct fails (§4.4). The relay is the
   guarantee: if `ssh <login>` works from the operator's computer, the restore works.
2. **Backups stay on HTTPS.** Backing up over this way would need a login for the client's host
   key on the store server's port 22. Not built; add later only if a client must back up while
   the HTTPS address is down.
3. **Several stores in one restore**: `--from-store` is repeatable, each matched to destinations
   by `url`.

## Files that change

`cli/src/commands/backup_store/{mod,report,rescue}.rs` (+ `list.rs`),
`nix/services/backup-store/{default,option}.nix` (store.json fields, report permissions, texts),
the portal templates, `cli/src/utils/backup/{source,access,restore}.rs`,
`cli/src/commands/restore/{mod,remote}.rs` and `local/`, `cli/src/commands/backup*`,
`docs/design/backup-restore.md` §4 and §6, `backup-store-rescue.md` §3–4, `docs/RESTORE.md`,
`docs/CLI.md`, `agent/skills/neo-admin` (+ `references/cli.md`), `nix/services/backup*/skills.nix`.

## As built

Differences from the text above:

- **Host key.** `store.json` gets `sshPort` and `hostKeyFile` (the path of
  `core.secrets.identityFile` + `.pub`), not the `hostKey` line: the key is made at install, after
  the evaluation. `neo backup-store list` reads it at run time.
- **report.json** was already 0644 in a 0755 folder: no permission change.
- **Option order.** `Access` puts the destination's own ssh options before its defaults (ssh takes
  the first value), so the pinned key (`HostKeyAlias` = the store's HTTPS host,
  `StrictHostKeyChecking=yes`, `known_hosts.store` in the work folder) wins over `accept-new`.
- **Matching.** A store destination matches when its `storeUrl` equals the store's `url`, a
  trailing `/` ignored. A picked backup that no destination names goes first.
- **`ssh+store://` with `--host`.** It becomes a route like `--from-store`, so on the new machine
  the settings' destinations follow it as fallbacks (without `--host` it stays the only source,
  like any `--from-backup`). A login written `user@host:port` becomes `ssh://user@host:port`.
- **The new machine's routes** go to `/root/neo-restore/store-routes.json`, read by the hidden
  `--store-routes`. Direct: `/root/neo-restore/store-<i>/ssh_config` with the hops as
  `neo-hop-<i>` (your known_hosts lines renamed, `@revoked` / `@cert-authority` lines dropped), one
  master `<client>.sock` per client, 40 s for the master and 20 s for the probe; a login reached
  through a ProxyCommand cannot be carried and goes to the relay. Relay: the new machine's port is
  random in 30000–59999, three tries. The masters close with `-O exit` after the restore.
- **§5 rescue: no code change.** The rescue never used the tunnel itself (it takes the config
  repo from the local copy); only the new machine reads over HTTPS. The portal's client page says
  so while `backup-store-tunnel` is not active, opens the "Over SSH to this server" tab and shows
  an alert.
- **§6 plan check.** Plugin URLs and `flake.lock` inputs (git, tarball, file, mercurial URLs;
  github, gitlab, sourcehut with a `host`) whose host does not resolve or answer on its port
  (5 s TCP connect) are a warning, not an error. Neo has no Nix cache setting yet, so caches are
  not checked.
- **A dropped relay** does not continue where it stopped: step 6 first tries the next
  destination (for example the store's HTTPS way), and a rerun of the command runs Disko again
  and copies from the start. The message says "run the same command again (it starts the copy
  over)".
- **Not tested end to end yet:** `--host` with `--store-via direct` and `relay` (§8 "Real"). The
  VM test covered `neo backup-store list`, `neo backup list --from-store`, `extract-config` over
  SSH with `backup-store-tunnel` stopped, a wrong host key and a key that is no restore key.
