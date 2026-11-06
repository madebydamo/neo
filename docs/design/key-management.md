# Design: Key Management, a clean Nix store, encrypted backups and the recovery kit

Status: proposal (2026-11-06) · nothing implemented

One page answers: who can get in, who can read the data, what is still exposed, and how do I get
everything back. The same page holds the controls for encryption, cleanup and the recovery kit.

Builds on [secrets.md](secrets.md) and [backup-restore.md](backup-restore.md).

## Goals

- **Key Management tab** under Settings, replacing the Neo CLI card and Access & backup.
- **Separate SSH access from backup and restore keys**, with one-click suggestions to connect them.
- **No secret reaches the Nix store**, and old copies can be found and removed.
- **Encrypted backups as an informed choice:** every format stays available, the page warns while backups
  aren't encrypted and helps you decide.
- **Remove secrets from git history**, only when a clean, working state remains.
- **Recovery kit:** a zip with instructions and binaries to get the data back without Neo.

**Non-goals:** disk encryption (see "Parked" at the end), protecting against an attacker on the running machine,
hardware-backed decryption keys, sharing secrets across machines.

**What this protects:** copies of your config, backups stored elsewhere, and secrets from other local
users and remote builders. **What it doesn't:** the machine's disk stays unencrypted, so someone holding
it can read the data and the host key, and the host key opens the config repo's secrets and the
encrypted backups. The page says so.

## 1. Settings layout

The grid becomes **System · Disks · Key Management**. Power stays on the Settings page.

`[neo-cli]` keeps its place in `settings.toml`, so no migration is needed. It renders as a collapsed
**Neo CLI** group on the System pane:

- `extract_pane` adds the `neo-cli` options to the `core` pane, named `neo-cli.<option>`.
- The pane saves to `/save-core/core?neo_cli=true` only when that extract succeeded. That save replaces
  `[neo-cli]` whole, so a failed extract can't wipe the section.
- `core.neo-cli` joins `REFUSED` in `proposals.rs`, because the assistant's core lookups now see these options.
- `/configuration/core/neo-cli` shows the System pane.

New route `/configuration/keys`:

| Section | Content |
|---------|---------|
| Overview | One status row each for secrets, Nix store, git history, backups, recovery, disk |
| Who can log in | `core.ssh.authorizedKeys`: label, SHA256 fingerprint, add, remove |
| Backup and restore keys | `core.secrets.recipients` plus the host key, with quick-add suggestions |
| Encryption | Encrypt, Decrypt, Re-encrypt, escrow status |
| Cleanup | Git history and Nix store: what was found, exposed settings, cleanup actions |
| Backups | Warning while not encrypted, format comparison, Switch action, last run, "None configured" if empty |
| Recovery kit | Build, download, last built, out-of-date warning |
| Connect | Copyable commands: `ssh`, host fingerprint, `sops edit`, `neo restore`, `neo backup extract-config`, test my key |
| Add a key | Tabs for Linux, macOS, Windows, Android, iOS |
| How it works | Plain-language explanation of the chain in §7 |
| This machine's keys | Homeserver and git keys (they log in *elsewhere*), `settings.toml` download and upload |

Existing cards (`routes/ssh.rs`, `routes/secrets.rs`) move here. All setting writes go through
`settings::edit::apply_edit`. The status and scans load lazily with htmx, because they are slow.
`ssh-keygen` joins neo-web's closed `PATH` for fingerprints.

The disk row only informs: it reads `zfs list -o name,encryption`, or `/proc/mounts` and dm-crypt for
other filesystems, and reports "Not encrypted" with the limit above. It isn't a to-do.

Every key gets a capability row:

| Key | Log in | Open backups and restore |
|-----|--------|--------------------------|
| Laptop (ed25519, both lists) | yes | yes |
| Phone (`sk-` key, SSH only) | yes | no: this key type can't |
| Recovery key (recipient only) | no | yes |

### Help: adding a key

One tab per device, each with: create the key, copy the public key, paste it here. Each states whether
the key can open backups and restore this server. Keys that can log in but never do that: FIDO (`sk-`), RSA, ECDSA, Secure Enclave, and
any key whose private half can't be exported to a file. The help recommends one offline recovery key in
addition to daily keys.

## 2. SSH keys and backup and restore keys

The page presents two jobs: **log in to this server**, and **get everything back**. A backup and restore key opens
encrypted backups (`neo-backup.age`) and rebuilds the machine (`identity.sops.yaml`, `secrets.sops.yaml`). That's what
people need these keys for, so the page leads with it; decrypting the secrets file is how it works underneath.

New setting `core.secrets.recipients` (ssh-ed25519 only, labelled). It keeps that name in settings; its label is
"Backup and restore keys".

- **Migration:** seed it with today's ed25519 `authorizedKeys` so nobody loses access.
- **New SSH keys only log in.** An ed25519 key that isn't a recipient yet appears as a suggestion with an
  Add button. Other key types show why they can't be added.
- **Consumers to switch:** `admin_keys` and `recipients` in `secrets_store.rs`, `.sops.yaml` generation,
  `escrow`, `needs_rekey`, `neo-backup.age` (`nix/services/backup/default.nix:25`), restore messages, docs.
- **Guards:** refuse to remove the last recipient. Warn with only one recipient, or when every recipient is
  also on this machine. Refuse both lists in agent proposals (`proposals.rs:46`).

**What the split does not do.** `admin` and `homeserver` are in `wheel` and `docker`, so any SSH key can
become root on the running machine, read the host key and decrypt. The split protects offline copies (repo,
backups) and allows recovery keys with no login. The page says this next to the list.

**Removal is not revocation.** Old ciphertext still opens with a removed key. Removing a recipient offers the
"lost device" flow: remove the key from both lists, re-encrypt, change the exposed secrets, run
`neo backup rotate-key`, rebuild the recovery kit.

## 3. Keeping secrets out of the Nix store

`/nix/store` is readable by every local user, and its contents can travel to remote builders.

### How secrets get there today

| Route | Why | Fix |
|-------|-----|-----|
| **Inline or plain backend** | The config repo is the flake source, so `settings.toml` and `secrets.yaml` are copied into the store whole. Template text also holds the real values. This is the default on a new machine | Encrypt. Only the sops backend keeps values out |
| **Every evaluation** | neo-web evaluates the working tree, so a saved value should land in the store without any activation, in a path no generation references. **Confirm in the test below** | Encrypt; cleanup covers these paths |
| **Old generations** | They keep the plaintext from before encrypting | Cleanup (§5) |
| **Plain plugin options** | Options declared with plain `mkOption` stay in `settings.toml` on any backend | Migrate plugins to `mkSecretOption`; the scan flags them |
| **Password hashes** | `core.hashedLinuxPassword` and `services.tinyauth.users` are plain options, so hashes are world-readable, and a weak password like `asdf` falls to brute force quickly | Make them secrets, see below |
| **Remote builders** | Inputs of a remote build get copied to the builder | Follows from the fixes above |

An audit of repo options by name found no other plain secret options. It only matches single-line
declarations with secret-like names, so the lint and test below replace it.

### Leak guards

All of these are in scope. Each catches a leak at a different point:

| When | Guard | What it does |
|------|-------|--------------|
| Writing code | **Lint** | `secrets.md` §2.5 planned one, but it doesn't exist. `just check` fails on an option with a secret-like name that isn't declared with `mkSecretOption`, unless it's on a short allowlist (`tokenUrl`) |
| Building | **Wider placeholder guard** | The assertion in `core/secrets.nix` only checks container and unit `environment` and `environment.etc` text. Extend it to unit scripts (`script`, `preStart`, `postStart`), `serviceConfig`, container `cmd` and `environment.etc` sources, so a secret used outside a template is a build error there too |
| Testing | **Canary test** | A VM test (`just test-store-leak`) sets a unique value in every path from `extract_secret_paths`, encrypts, saves and activates, then searches the store for each value. It must find none. It also confirms the evaluation route above by checking a value saved while still plain |
| Activating | **Activation warning** | Runs the §4 detector on the tree about to be activated. If it would put secrets in the store, Activate says which settings and links to Encrypt. It warns but doesn't block, since unencrypted is a valid choice |
| Risky actions | **Action warnings** | Decrypt, Move to `secrets.yaml` and `settings.toml` upload say first that they bring plaintext back (§4) |
| Running | **Scans and status** | The store scan below and the history scan (§4) feed the Overview, so an existing leak stays visible |

Password hashes and new machines:

- **Hashes as secrets:** both become `mkSecretOption`. Two problems need solving first.
    - The secrets YAML reader rejects lists, and `tinyauth.users` is a list. Store it as one newline-joined
      secret, with a migration. The form keeps one row per user.
    - `hashedLinuxPassword` feeds `users.users`. `hashedPasswordFile` is read during the `users` activation step,
      but neo-secrets currently runs after it, so decrypting must move before that step while rendering (which needs
      the users to exist) stays after it. A failed decrypt must never lock the account out of a working login. Prove
      this in a VM, including a first boot and a switch from an old generation.
- **New machines:** still start inline, because encrypting needs an ed25519 key. The Overview shows a prominent
  Encrypt action as soon as one exists.

### Store scan

The page checks the real store, using the detector from §4:

1. List `/nix/store/*-source` and read `settings.toml` and `secrets.yaml` in each, which covers flake sources
   whether or not they're still in use.
2. Report the count, names of exposed settings (never values), and whether each copy is held by a generation
   or is free to collect.

Status row: "Clean", or "N old copies hold secrets" with the cleanup action.

## 4. Git history

### Detection

Shared with the store scan. A tree leaks if any of these hold, checked per unique tree across `HEAD` and
`activation_*`:

- `settings.toml` has a non-empty value at a secret path (`extract_secret_paths`).
- `settings.toml` has a non-empty value whose key looks secret (`*password*`, `*token*`, `*secret*`, `*apiKey*`),
  which catches plain plugin options and removed options.
- `secrets.yaml` exists with values.
- The tree contains a current secret value verbatim, in any tracked file. This needs decrypted values, so it runs
  through `neo-secrets-root` and keeps only hashes in memory.

Results list **setting names, never values**. Encrypted files count as clean.

These actions can bring plaintext back, so they warn first: Decrypt, Move to `secrets.yaml`, and `settings.toml`
upload.

### Conditions

Cleanup is available only if a **safe base** exists: a clean commit with `(generation N)` in its message, where
generation N still exists on the system. `HEAD` must also be clean, the tree must not be dirty, and the running
generation must come from a clean commit.

If the newest safe base is under 7 days old, show a warning and require a tick to continue: "Wait about a week, so
you know this version works." It's a hint, not a block.

Otherwise the action is disabled with the reason, because the alternative is erasing all version history.

### Operation

Takes the `system` exclusive lock.

1. Rewrite in a temporary clone. Each leaking commit is dropped and its children are re-parented to the nearest
   clean ancestor. Trees are snapshots, so remaining commits keep their contents. Messages and dates stay
   unchanged, so `(generation N)` links survive.
2. Verify: the scan comes back empty, the `HEAD` tree is identical, and every kept generation still resolves to a
   commit.
3. Swap in the new refs. Delete `activation_*` branches with no remaining commit.
4. Remap `version-notes.json` to the new hashes. Expire the reflog, then `git gc --prune=now`.

Generations whose commit was dropped still boot and roll back, but Versioning shows them with no config commit.

## 5. Cleanup: what's left after the rewrite

The result screen lists each remaining copy with its next step.

| Copy | Next step |
|------|-----------|
| **Nix store** | Separate confirmed action, same safe base and 7-day hint: delete generations older than the safe base, then garbage collect, then scan again. Reports anything still held by another root, such as a `result` link. `system-updater` already deletes after 30 days by default, but only when auto-update is on and not for roots elsewhere |
| Remote (`origin`) | Shows the force-push command. The host may keep old commits; other clones keep them |
| ZFS snapshots of `/var/neo` | The repo lives there. Lists affected snapshots with a link to delete them; never automatic, because it removes restore points for all data |
| Backups | Lists destinations. Old copies age out with retention, except plain `files` backups, which need a manual clean |
| The values themselves | If the repo was ever pushed, treat them as exposed: list them and offer Generate where available |

The store action needs a new `mkSudoExtraRules` entry for deleting generations. Rollback to anything older than
the safe base is gone afterwards, and the confirmation says so.

## 6. Encrypted backups

Defaults don't change: `format` stays `files` and no migration touches existing backups. The page makes the choice
visible instead.

- **Warning:** while backups aren't encrypted, the Overview's backup row and the Backups section show a warning: anyone
  with access to the destination can read your data and old config versions. "None set up" warns too.
- **Help deciding:** the Backups section compares the formats in plain words, and marks the current one.

| | Plain files | Encrypted | ZFS snapshots |
|---|---|---|---|
| Someone at the destination can read it | Yes | **No** | Yes |
| Keeps older versions | No | Yes | Yes |
| Destination needs | Any computer or disk | SFTP or any disk | ZFS on both sides |
| To restore you need | A copy tool | A backup and restore key | ZFS (live USB) |
| Good when | The destination is your own and physically safe | The destination is someone else's, or could be lost | You own both ends and want speed |

- **Losing keys matters:** the comparison states the trade-off: with Encrypted, losing every backup and restore key means
  the backup can't be opened. If there's only one key, it prompts you to add another first.
- **Switching:** a confirmed action that sets `format`, saves and asks to activate. It starts a new full backup, and it
  checks the destination allows SFTP first. The old plain copy is **never deleted automatically**, because it may be the
  only backup until the new one finishes. The page then shows where it is and reminds you to remove it.
- **Keys:** `neo-backup.age` follows `core.secrets.recipients` (§2), and Connect shows the commands to open a backup and
  restore with one of them.
- **Login key:** backups log in with the host key by default. Someone holding the disk could also log in to the
  destination and delete backups, so the Backups section recommends append-only or a restricted account on the
  destination. Details stay out of scope.

Restore already handles `encrypted`, including `--from-backup`.

## 7. The chain the page explains

```
your SSH key ──can open──► identity.sops.yaml (escrow of the host key)
      │
      ├──can open──► secrets.sops.yaml ◄──────can open─────┐
      └──can open──► neo-backup.age ──► restic backup      │
                         ▲                                 │
                         └──────can open──────── host key ─┘
                                                 (unencrypted on this machine's disk)
```

So a stolen copy of the repo or a backup is safe. A stolen disk isn't, and the page says that in one plain line.

## 8. Recovery kit

A zip that gets the data back on any 64-bit Linux. Recovery from macOS or Windows uses a Linux live USB, and
`START-HERE.html` explains how.

### What goes in

```
neo-recovery-<host>-<date>.zip
├── START-HERE.html          # steps for this machine, works offline
├── recover.sh               # guided: pick key → pick backup → extract
├── bin/<arch>/              # static: restic age sops rsync neo
├── config/config.bundle     # git bundle of the clean config repo
├── keys/                    # ciphertext only
│   ├── secrets.sops.yaml
│   ├── identity.sops.yaml
│   └── neo-backup.age
└── manifest.json            # versions, sha256 of every file, build time, recipient fingerprints
```

`START-HERE.html` is generated from this machine: backup destination and format, repo URL, host and disk layout,
recipient fingerprints ("you need one of these keys"), and neo version.

### Two levels

| Level | Offline? | What it does |
|-------|----------|--------------|
| **1. Get the data** | Yes, except reaching an SSH backup target | Decrypts and extracts config, secrets and appdata into a folder |
| **2. Rebuild the machine** | No | `neo restore` needs Nix, nixpkgs and flake inputs from the internet. The kit gives a pinned command and the NixOS ISO link |

A fully offline rebuild would mean a multi-GB image with the whole system closure, so that's a possible later
"recovery ISO" rather than a zip.

Tool needs per backup format at level 1: `encrypted` needs `restic` and `age`, both in the kit. `files` needs only a
copy tool. `zfs` can't work from a portable binary because it needs a matching kernel module, so its instructions use
the NixOS live ISO, which includes it.

### Binaries

Plain nixpkgs binaries can't be copied out. Checked on this machine: `restic`, `age` and `sops` are dynamically linked
or wrappers tied to `/nix/store`.

- Package `neo-recovery-tools` with `pkgsStatic` builds. Build `restic`, `age`, `sops` and `rsync` for the machine's
  architecture (x86_64 or aarch64).
- `neo` has no native C crates in `Cargo.lock`, so a musl static build looks feasible. Confirm that it builds, and that
  `restore` and `extract_config` work with tools from `bin/`.
- **Build on request, not in every system closure**, to avoid adding about 100 MB to every machine. First build may need
  compiling. Show progress with the op monitor.
- Add a `neo recover` subcommand rather than a long shell script, reusing `restore/backup.rs`.

### Rules

- **Never contains a private key or plaintext secret.** A kit is useless without a recipient key, so it's stored apart
  from that key. If secrets aren't encrypted, building is refused with a link to Encrypt.
- **The history in the bundle must be clean.** With leaks present, use only clean commits, or refuse if there are none.
- **Metadata is readable:** hostnames, backup addresses, usernames, service list. The download warns not to publish it.
- **Treat a downloaded kit as permanent:** it keeps opening with the keys it was built for. After removing a recipient,
  replace all copies.
- **Staleness:** record a hash of the inputs and show "kit is out of date" after a recipient, backup key or backup
  destination change. `neo-backup.age` at the destination is authoritative because it is re-encrypted every run, so
  `recover.sh` prefers that one and falls back to the kit's.
- **Download:** `system` shared lock, journal entry, not available to Hermes.

### Recovery drill

A kit nobody has tested isn't a recovery plan. `recover.sh --check` confirms the key opens each file, reaches the backup
and lists a snapshot, changing nothing. The Overview's recovery row shows the last time the operator confirmed a drill.

## 9. Rollout

Each phase is usable alone.

| Phase | Deliverable | Depends on |
|-------|-------------|------------|
| 1 | Tab, moved cards, Neo CLI group, read-only status, Connect, Add a key, How it works | — |
| 2 | `core.secrets.recipients`, migration, quick add, capability table, lost-device flow | 1 |
| 3 | All leak guards (§3), store scan and status, password hashes as secrets | 1 |
| 4 | History scan and rewrite, store cleanup, remaining-copies screen | 3 |
| 5 | Backup warnings, format comparison, Switch action | 2 |
| 6 | Recovery kit: static tools, `neo recover`, zip, staleness, drill | 2, 5 |

Update `agent/skills`, `docs/INSTALL.md`, `docs/RESTORE.md`, `docs/CLI.md` and `AGENTS.md` with each phase, including
`Settings → Secrets` and the offsite-backup guide's `Access & backup` mention. Phase 2 gets a VM test of a lockout,
phase 4 of a rewrite, and phase 6 restores from a kit on a clean VM with the network limited to the backup target.

## Decisions

- **Disk encryption is parked** (below).
- **Backup defaults stay.** `files` remains the default; the page warns and helps you choose (§6).
- **Password hashes become secrets**, including `hashedLinuxPassword` (§3).
- **All leak guards are in scope** (§3). They warn rather than block, except the lint and build guard.
- **The page frames keys around backups and restore** (§2).
- **History cleanup covers the local repo.** Remote, snapshots and backups are listed with next steps (§5).
- **New SSH keys only log in** until added as a recipient through a suggestion (§2).
- **Recovery from macOS and Windows uses a Linux live USB.** The kit ships Linux binaries only.

## Parked: disk encryption

Not planned now. Without it, a stolen disk exposes the data and the keys, as stated above. Conclusions so far, for when
this comes back:

- ZFS can't encrypt an existing dataset. It needs a copy to a new dataset, and old plaintext stays on the disk until
  overwritten, so a wiped or new disk is the only certain way.
- Encrypting only `/var/neo` is enough if the decryption key, homeserver key, git key and backup login key move there.
  The host key can stay on root once it can no longer decrypt.
- A TPM allows automatic reboots and protects a disk taken without its motherboard. It doesn't protect a whole stolen
  machine, because the root stays unencrypted and unverified and the machine unlocks itself.
- Protecting a whole stolen machine needs either manual unlock after each reboot, or encrypted root with Secure Boot.
  Secure Boot conflicts with today's GRUB and BIOS setup.
