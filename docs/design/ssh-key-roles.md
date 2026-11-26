# Design: people keys and machine keys

Status: implemented (2026-11-26)

Replaces §2 of [key-management.md](key-management.md) (a separate `core.secrets.recipients`
list). Builds on [secrets.md](secrets.md) §4 and [backup-restore.md](backup-restore.md) §3.

## Answer

Two lists of SSH keys:

| Setting | For | Logs in as | Opens backups and secrets |
|---------|-----|------------|---------------------------|
| `core.ssh.authorizedKeys` | people | root, admin, homeserver | yes (ssh-ed25519) |
| `core.ssh.machineKeys` | other computers (Nix builders, automation) | homeserver | never |

A key in `authorizedKeys` is always a decryptor. Nothing else to add, so you cannot forget
a person. A machine key goes into its own list on purpose.

## Problem

`core.ssh.authorizedKeys` did two jobs: SSH login, and recipient of `secrets.sops.yaml`,
`identity.sops.yaml` and `neo-backup.age` (plus read-only access at a backup store through
`neo-backup.recipients`). A builder key such as `root@brugg` could therefore decrypt every
secret and every encrypted backup copy of this server.

A second, hand-kept list of decryptors would fix the leak but make the bad mistake easy: add a
new laptop, remove the old one, and no key you own opens the backups. Losing access is worse than
an extra decryptor, so the step you can forget must be the one that restricts, never the one that
keeps restore working.

## How it works

- **Login:** `homeserver` gets `authorizedKeys ++ machineKeys` (`nix/modules/core/default.nix`).
  `root` and `admin` keep `authorizedKeys` only (`nix/modules/core/base.nix`).
- **Decryption:** unchanged. sops (`cli/src/utils/secrets_store.rs` `admin_keys`) and the backup
  header (`nix/services/backup/default.nix` `adminKeys`) read `authorizedKeys` only. Machine keys
  never reach them.
- **Password login** follows `authorizedKeys` alone. A server with machine keys and no person key
  keeps password login, so the console is not locked out.
- **One key, one list:** an assertion fails when a key (type and base64, comment ignored) is in
  both lists.
- **Assistant:** `core.ssh.machineKeys` is in `REFUSED` (`proposals/check.rs`), like
  `authorizedKeys`.
- **Access page:** machine keys appear under SSH with a `machine` badge
  (`neo.exposure.report.ssh.machineKeys`).

`homeserver` is in `wheel` and `docker` and is a Nix trusted user, so a machine key can manage the
server and become root on the running machine. That is intended. The split protects the copies
elsewhere: the config repo, the escrowed host key and the encrypted backups.

## Moving a key from people to machines

1. Remove it from Authorized SSH keys and add it to Machine SSH keys, in one save. The web UI
   save re-encrypts the sops files; by hand, run `neo secrets rekey`.
2. The next backup run writes `neo-backup.age` and `neo-backup.recipients` without the key, and a
   backup store drops its read-only access within 15 minutes.
3. **Removal is not revocation.** Old backup headers and old commits of `secrets.sops.yaml` still
   open with the key. If that matters, run `neo backup rotate-key` and change the secrets.
