# Design: runtime secrets, sops encryption and machine recovery

Status: phases 1–2 implemented (2026-10-28); phases 0, 3–5 open · see "Implementation status" at the end

A machine's whole configuration can live in a git repo on GitHub, and an admin's SSH key is
enough to rebuild the machine from it. Secrets never reach `/nix/store` (once encrypted), and
each machine has **one** ed25519 key that is its SSH host key, its git identity and its
decryption identity.

## Goals

- **Runtime secrets:** every secret option resolves at activation to a file under
  `/run/neo-secrets`, never to a value baked into a unit.
- **Two backends, one layout:** `plain` (an unencrypted repo, the same security level as
  today) and `sops` (an encrypted repo). Services and lib helpers can't tell them apart.
- **One machine key:** a single ed25519 key is the SSH host key, the machine's SSH client key
  and the age identity.
- **GitHub over HTTPS tokens:** the machine talks to GitHub (config repo, private plugins) with
  `core.git.tokens`, which is itself a secret. The SSH key is only a fallback for special setups.
- **Self-service bootstrap:** `neo secrets enable` on a running field machine converts it in
  place. It needs no admin key, because the machine already holds its own.
- **Recovery:** `neo restore <repo>` on a fresh NixOS. An admin SSH key decrypts the machine
  key, and the machine reinstalls with the **same identity**.

**Non-goals:** disko/hardware config, restoring appdata (that's the backup's job; the backup
password is just another secret), and secrets shared across machines (v1).

## Overview

```
                   settings.toml (plain, no secrets)        ─┐
 config repo  ───  secrets.yaml        (backend = plain)     ├─ git / GitHub
                   secrets.sops.yaml   (backend = sops)      │
                   identity.sops.yaml  (machine key, admins) │
                   .sops.yaml          (generated recipients)┘
                         │ eval: structure only (ciphertext / presence, never values)
                         ▼
 activation:  backend ──► /run/neo-secrets/raw/<option/path>           root 0400
                              │  neo-secrets render (templates, owner, mode)
                              ▼
              /run/neo-secrets/rendered/<name>  ──► environmentFiles / *File / bind mounts
```

The **backend** decides only how raw values arrive in `raw/`. Everything above it (layout,
templates, ownership, restarts) is the same neo layer for both backends.

---

## 1. One machine key (identity)

Today there are three separate keys:

| Key | Path | Used for |
|-----|------|----------|
| sshd host key | `/etc/ssh/ssh_host_ed25519_key` (NixOS default) | host identity |
| homeserver key | `/home/homeserver/.ssh/id_ed25519` (`lib.neo.defaultHomeserverSshKey`, `base.nix`) | backup targets, remote builds |
| git key | `/var/lib/neo/git/id_ed25519` (`lib.neo.gitSshKey`, `core/git.nix`, root:neo-git 0640) | git over SSH (fallback), Hermes |

The new design has **one** identity, `/var/lib/neo/identity/ssh_host_ed25519_key`
(root:root 0600, `.pub` 0644). It is used as follows:

- **sshd:** `services.openssh.hostKeys = [{ path = …; type = "ed25519"; }]`. Drop the RSA host
  key.
- **sops-nix:** `sops.age.sshKeyPaths = [ <identity> ]`. sops-nix derives the age X25519 key from
  it at activation.
- **git and the homeserver user:** they use it through an **ssh-agent socket, never the file**.

### Why an agent instead of a group-readable file

The git key is readable by every member of `neo-git` today, and that includes **Hermes**. If the
unified key stayed readable the same way, every member could also **decrypt every secret** and
impersonate the host.

So `neo-identity-agent.service` runs as root:
`ssh-agent -D -a /run/neo/identity-agent.sock`, followed by `ssh-add <identity>`. The socket is
`root:neo-git 0660`. The `Match … localuser …` block in `core/git.nix` swaps
`IdentityFile` for `IdentityAgent /run/neo/identity-agent.sock`.

- Members can **sign** (git push, ssh to backup targets), exactly as much as today.
- Members **cannot** read the private scalar, so they can't do the X25519 DH that age needs.
  Decryption stays root-only.
- nix-daemon (remote builds) runs as root and may use the file directly.

`mkSshKeyScript` stays for the generate/rotate logic, but it targets the identity path. A
"rotate identity" button becomes a heavyweight operation: it re-keys sops (§4) and changes the
host key clients see. GitHub isn't affected, because it uses tokens (§1.1).

### Generation

The key is generated the same way in both backends: the first activation creates it if it's
missing (the NixOS sshd-keygen behavior, pointed at the new path). The **only** difference is
escrow:

- **sops:** the identity is also stored in `identity.sops.yaml`, encrypted to the admin keys only.
  This is what makes recovery possible.
- **plain:** no escrow (it would be plaintext in git). A restore produces a new identity, so
  clients see a new host key.

So no different key generation is needed for encrypted repos. Escrow is the only addition.

### Migrating field machines (`neo identity unify`)

This runs as step 0 of `neo secrets enable`, or on its own.

1. Copy `/etc/ssh/ssh_host_ed25519_key` to the identity path. Clients' `known_hosts` stays valid.
2. Switch: sshd, agent and git config all point to the identity.
3. The git and homeserver keys **change** to the host key. Print the new public key and tell the
   operator to add it to backup targets' `authorized_keys`, to remote builders, and to any git
   host still reached over SSH (the fallback, §1.1).
4. Transition window: the old git and homeserver keys stay as extra `IdentityFile`s (loaded into
   the agent too) until the operator clicks "retire old keys" or N days pass. The web UI shows a
   warning while they're still in use.

### 1.1 GitHub access: tokens first, SSH as fallback

The machine reaches GitHub over **HTTPS with a token** from `core.git.tokens` (e.g. a fine-grained
PAT, scoped to the config repo with contents read/write, plus read access to private plugin
repos). The plumbing already exists in `core/git.nix`: the credential helper for git over https
and the Nix `access-tokens` include. Phase 1 only moves the token into the secrets file and the
rendered `/etc/neo/git/*` files.

Why tokens:

- Changing or restoring the machine identity never touches GitHub, so there are no deploy keys to
  swap.
- One credential covers git and Nix flake inputs (`github:` refs can't use SSH keys).
- Scope and expiry are managed in GitHub, and the token travels with the encrypted secrets, so a
  restored machine can push right away.

The SSH identity (through the agent) stays available for git hosts reached by `git+ssh://`
(self-hosted Gitea, special setups). neo never relies on it for GitHub.

Fine-grained PATs expire. The web UI warns before expiry when the API reports it (open question).

---

## 2. Phase 1: secret options become runtime files (no encryption yet)

This is the bulk of the work (~35 options in ~21 services today: authentik, hermes, karakeep,
activepieces, vpn, paperless, docmost, dawarich, beszel-agent, rathole, dufs, `core.git.tokens`, …).
It is useful on its own: once `/run` holds the secrets, encryption is just a backend swap.

### 2.1 Declaring: `lib.neo.mkSecretOption`

```nix
# option.nix (same extra keys as lib.neo.mkOption: rank, helper, ui)
password = lib.neo.mkSecretOption {
  default = "";
  description = "WebDAV password";
  rank = 20;
};
```

A secret option is still a **plain string option**, tagged `secret = true`. Its value is:

- **the secret itself** on an unencrypted config (inline in `settings.toml` today, `secrets.yaml`
  after phase 2). That's the same exposure as today, and every field machine keeps working
  unchanged;
- **a placeholder** `@NEO_SECRET:<path>:<stamp>@` once the value comes from `secrets.sops.yaml`.
  `<path>` mirrors the settings path (`services/dufs/password`), and `<stamp>` is a hash of the
  ciphertext, so it changes whenever the value does. `settings.nix` puts the placeholder in place
  of the value.

Because a set secret is always a non-empty string, eval-time checks like `cfg.password != ""`
keep working in both modes. No handle type and no migration of existing settings is needed.

Extract passes `secret` through to the form, which renders a masked input with a Show toggle.

### 2.2 Consuming: `neo.core.secrets` (`nix/modules/core/secrets.nix`)

A secret only ever reaches a service through a **template**: text in which secret values (or
their placeholders) appear anywhere. It is rendered at activation with an owner and mode, and it
exposes a `path`.

```nix
# 1. OCI env vars (most services): a rendered env file, added to the container's
#    environmentFiles, restarts docker-<container> when it changes.
neo.core.secrets.containerEnv.dufs = {
  DUFS_AUTH = "${cfg.username}:${cfg.password}@/:rw";
};

# 2. Whole config files (rathole toml, app config, nginx snippets)
neo.core.secrets.templates."rathole-client.toml" = {
  content = configText;                 # cfg.token interpolated as usual
  restartUnits = ["rathole.service"];
};
serviceConfig.LoadCredential =
  "config.toml:${config.neo.core.secrets.templates."rathole-client.toml".path}";

# 3. Native NixOS *File options: a template whose content is the value
neo.core.secrets.templates."vikunja-db-password" = {
  content = cfg.dbPassword;
  owner = "homeserver";
  restartUnits = ["vikunja.service"];
};
```

Template options: `content` or `env` (attrs → `KEY=value` lines), `owner`/`group` (default
root), `mode` (default 0400), `restartUnits`, and `path` (read-only). Rendered files live in
`/run/neo-secrets/rendered/` (tmpfs), never in appdata. Apps that rewrite their config file are
the exception: they need a `mkSetupService` that copies the file into appdata (decided case by
case). `DynamicUser` services read root-owned templates through `LoadCredential`.

`/run/neo-secrets` is a root-owned directory (0751) of its own. `/run/neo` belongs to homeserver,
which could swap a subdirectory out from under root.

**Leak guard:** an assertion scans `virtualisation.oci-containers.containers.*.environment`,
`systemd.services.*.environment` and `environment.etc.*.text` for `@NEO_SECRET:`. A placeholder
outside a template is a build error, never a silent literal. In plain mode there are no
placeholders, so unmigrated services keep working. In sops mode the guard blocks the switch until
every service that consumes a secret goes through a template, which is the precondition for
`neo secrets enable`.

### 2.3 Rendering: `neo-secrets` activation

`system.activationScripts.neo-secrets` runs on every switch and boot (deps: `users`, `groups`,
`specialfs`, plus `setupSecrets` in sops mode):

1. The backend has already filled `/run/neo-secrets/raw/` (sops-nix in sops mode; nothing to do
   in plain mode, where the values are already in the template text).
2. For each template, substitute each placeholder with `raw/<path>`, then write atomically
   (`mktemp` → chown/chmod → `mv`). Files of templates that no longer exist are removed.
3. A missing raw value for a placeholder fails the activation and names the setting. A partly
   rendered system is worse than a failed switch.

### 2.4 Restarts

The store source of each template is a `restartTrigger` of its `restartUnits`. The source holds
the value (plain) or a placeholder with the ciphertext stamp (sops), so changing a secret
restarts exactly the units that use it. No runtime diffing is needed.

### 2.5 Things that can't simply move

| Pattern today | Migration |
|---------------|-----------|
| `environment = { X = cfg.secret; }` | `neo.core.secrets.containerEnv.<container>` |
| Secret inside a generated config file | `neo.core.secrets.templates` |
| `environment.etc."…".text = token` (`core/git.nix` tokens, access-tokens.conf) | a template with owner `root:neo-git`; nix.conf already `!include`s it |
| Branch on "is it set" | unchanged (`!= ""`): a placeholder is non-empty |
| Derived values (bcrypt/htpasswd, e.g. tinyauth users) | stored as the finished string, as today (`user:$2a$…`). The web UI helper hashes on input; no template filters |
| Secret needed **before eval** (private plugin inputs) | rendered access-tokens `!include`; during `neo restore`, passed as `NIX_CONFIG` from the decrypted file |

A lint in `just check` warns on any `types.str` option named `*password*|*token*|*secret*|*apiKey*`
that isn't a secret type, so new services can't regress.

---

## 3. Settings split and the plain backend

### 3.1 Files

| File | Content | Git |
|------|---------|-----|
| `settings.toml` | everything that isn't a secret, plus `core.secrets.backend = "plain" \| "sops"` | yes |
| `secrets.yaml` | secret values, same paths as settings (example below) | yes (plain backend) |
| `secrets.sops.yaml` | the same document, sops-encrypted | yes (sops backend) |
| `identity.sops.yaml` | `ssh_host_ed25519_key: \|` (the private key), admin recipients only | yes (sops backend) |
| `.sops.yaml` | creation rules, **generated** from settings (§4) | yes |

```yaml
# secrets.yaml (plain) — secrets.sops.yaml has the same keys, values ENC[AES256_GCM,…]
services:
  dufs:
    password: c8e4f1a2…
  rathole:
    token: 1985c3fd…
core:
  git:
    tokens:
      github.com: github_pat_…
```

**YAML, not TOML:** sops can't encrypt TOML, and YAML is sops' and sops-nix's default format.
sops keeps the keys in cleartext, so eval only needs the **structure** of the file plus the raw
leaf text (`ENC[…]` in sops mode). That's where the placeholders and their stamps come from, without
decrypting anything.

**Nix has no `fromYAML`**, so `lib.neo.secrets.readKeys` is a small reader for the subset that
neo and sops write: nested maps with two-space indentation, scalar leaves, and block scalars
(`|`, `>`), whose content it skips. It stops at the top-level `sops:` metadata key. Anything else
(anchors, flow maps `{…}`, lists, tabs) is an eval error that names the line. That way a hand
edit can't silently turn into a missing secret. The CLI and the root helper write the plain file
with a small canonical emitter (sorted keys, quoted scalars, `|` for multiline), and they change
encrypted values with `sops set` / `sops unset`, which keep the layout.

Rejected: import-from-derivation with `yq` (IFD slows every eval and breaks `nix flake check`), and
a generated JSON sidecar (a second copy of the key list that can drift).

`templates/homeserver/modules/settings.nix` deep-merges `fromTOML settings.toml` with
`placeholders(readKeys secrets.sops.yaml)` (or the plain values of `secrets.yaml`), and stops putting secrets into `environment.etc."neo/settings.toml"`.

### 3.2 Is supporting both easy? Yes

The plain backend costs very little, and it's the stepping stone for the migration:

- **plain:** `secrets.yaml` is in the flake source, so it lands in the store, like
  `settings.toml` does today: **no worse than now, not better**. An activation snippet copies the
  values from the store path into `raw/` (root 0400) using `yq-go` at runtime. The rest of the pipeline is identical.
- **sops:** sops-nix with `defaultSopsFile = ../secrets.sops.yaml; defaultSopsFormat = "yaml";`.
  One `sops.secrets."<path>" = { path = "/run/neo-secrets/raw/<path>"; }` entry is generated per
  key in the file. Only ciphertext reaches the store.

That makes `neo secrets enable` a small step: encrypt `secrets.yaml`, flip `backend`, switch. The
rendered files come out byte-identical, so nothing restarts (§5 uses this as its verification).

Limits of plain that the docs should state honestly: the secrets are store-readable, there's no
identity escrow, and a restore gets a new identity.

### 3.3 CLI and web UI

- Extract marks secret options. The form renders a password input with a **set / not set** chip
  and the actions **Generate** (from `generate`), **Reveal** and **Clear**. Values never go into
  the page unless revealed.
- **Save** splits the payload: non-secrets go to `settings.toml` through `toml_edit` as today, and
  secrets go to the backend.
- In sops mode, writing a value means decrypting the data key, so it needs the identity, so it
  needs root. neo-web (homeserver) calls a privileged helper,
  `neo secrets set|unset|reveal <path>`, through `mkSudoExtraRules`. It takes the `system`
  shared lock like other settings writes. Reveal is logged to the journal.
- On the laptop (dev checkout), `neo init/build/edit` use the operator's own key
  (`SOPS_AGE_SSH_PRIVATE_KEY_FILE`, default `~/.ssh/id_ed25519`).
- `neo migrate` gets a step that moves inline secrets from `settings.toml` into `secrets.yaml`.
  The secret flags come from the extract metadata, never a hardcoded list.

---

## 4. Recipients and re-keying (sops backend)

Recipients come from data that's already in settings:

- `secrets.sops.yaml` → the machine identity **plus** every `core.ssh.authorizedKeys` entry of
  type `ssh-ed25519`.
- `identity.sops.yaml` → only the `core.ssh.authorizedKeys` entries. The machine doesn't need its
  own escrow, and this way the only route to it is an admin key.

So "has SSH access" = "can decrypt", which was the original requirement. `.sops.yaml` is
**generated** by neo from these lists and committed (sops needs it on disk). We track the latest
sops (nixpkgs 26.05: 3.13.3), which takes `ssh-ed25519` public keys as age recipients directly
and decrypts with `SOPS_AGE_SSH_PRIVATE_KEY_FILE` (default `~/.ssh/id_ed25519`). No `ssh-to-age`
step and no extra key files. The Phase 3 VM test confirms both directions.

**Re-keying is automatic.** Whenever a save changes `core.ssh.authorizedKeys`, the save handler
regenerates `.sops.yaml` and runs `sops updatekeys -y` on both files through the root helper,
inside the same `system` lock and the same commit. The machine can always do this because it's
always a recipient of `secrets.sops.yaml`. `identity.sops.yaml` has to be re-encrypted from the
identity file, which root can read.

**Only `ssh-ed25519` is supported.** Every other key type (RSA, ECDSA, `sk-ssh-ed25519@openssh.com`)
is skipped with a warning on the field: "this key can log in but can't decrypt or restore". Hardware
keys and extra age recipients are out of scope.

**Removing a key doesn't revoke it.** Old ciphertext stays in git history. The UI says so when an
authorized key is removed and offers the rotation list (§6).

---

## 5. Phase 3: `neo secrets enable` (field-machine bootstrap)

This runs **on the machine** (CLI or a web UI button) and takes the `system` exclusive lock, like
activate. No admin private key is needed.

1. **Preflight** (refuse if any fails): the config repo is clean, at least one authorized key is
   `ssh-ed25519` (otherwise nobody could ever restore), the identity is unified (otherwise run
   §1 first), and every secret option has migrated (no legacy inline values left, or step 2
   moves them).
2. **Split:** move any inline secrets from `settings.toml` to `secrets.yaml`. Record a snapshot of
   the current rendered files' hashes as the baseline.
3. **Encrypt:** generate `.sops.yaml`, then `sops -e secrets.yaml > secrets.sops.yaml` and
   `identity.sops.yaml`. Remove `secrets.yaml`. Set `core.secrets.backend = "sops"`.
4. **Switch** through the normal activate path.
5. **Verify:** every expected `raw/` and `rendered/` file exists, and the rendered hashes equal the
   baseline (same values, so no service restarted). The listed units are healthy.
6. **Commit** ("secrets: enable sops encryption"), and push if a remote is configured.
7. **On failure** at 4–5: switch back to the previous generation and `git reset --hard` to the
   pre-run commit. That commit still contains `secrets.yaml`, so nothing is lost.

**Afterwards (offered, not automatic):**

- Old system generations still hold the plaintext `settings.toml` / `secrets.yaml`. They're left
  to the normal automatic GC; no special purge.
- If the repo was ever pushed with plaintext, list the exposed secrets (from git history) and
  mark which ones are safe to rotate automatically (§6).

`neo secrets disable` is the reverse (decrypt → plain), and it's mostly there for debugging.

---

## 6. Phase 4: disaster recovery (`neo restore`)

On any NixOS live ISO with flakes enabled and the admin key available:

```
nix run github:madebydamo/neo -- restore git@github.com:me/box-config.git --target /mnt
```

1. Clone with the admin's own GitHub access (their SSH key or a token).
2. Decrypt `identity.sops.yaml` with the admin key, and write the identity to
   `/mnt/var/lib/neo/identity/` (0600).
3. Decrypt only what the install needs before eval (`core.git.tokens` → `NIX_CONFIG`
   access-tokens, for private plugin inputs).
4. Hardware: `neo generate-hardware` and disko are out of scope, as agreed.
5. `nixos-install --flake .#<host>`.
6. First boot: sops-nix decrypts with the restored identity, sshd presents the old host key, and
   git pushes with the restored token. Nothing has to be re-trusted.
7. Data: restore appdata from the backup. Its password is a secret that is now available (a
   separate future command plugs in here).

For a plain-backend repo, the same command works without step 2: a new identity is generated and
the restore prints the new host key to trust. The repo carries no token in plaintext, so the
operator enters one during the restore.

---

## 7. Security notes

- **The blast radius of an admin key** = every secret on every machine where it's a recipient,
  including git history. That's the same blast radius as SSH root access, and it matches the
  stated requirement.
- **Public repo:** the ciphertext is safe, but the metadata isn't: domain, service list, secret
  names and public keys. X25519 isn't post-quantum ("store now, decrypt later"). Prefer a
  private repo; the design works the same either way.
- **Local users:** in sops mode, `/nix/store` holds only ciphertext. `raw/` is root 0400, and
  `rendered/` is per-template owner on tmpfs.
- **Token and agent holders** (neo-git: homeserver, Hermes) can use the git token and sign with
  the identity (GitHub, backup targets), as today, but can't decrypt.
- **Removal ≠ revocation** (§4). Rotation is per option: `mkSecretOption { rotatable = true; }`
  marks secrets that are safe to regenerate (session keys) as opposed to ones that need the
  service to apply them (DB passwords).

---

## 8. Rollout

| Phase | Deliverable | Depends on |
|-------|-------------|------------|
| 0 | Unified identity + agent, `neo identity unify` | — |
| 1 | `mkSecretOption`, `neo.core.secrets` templates + renderer, leak guard, lint | — |
| 1b | Migrate services (one PR per few services; legacy inline keeps working) | 1 |
| 2 | `secrets.yaml` split, `neo migrate` step, web UI secret fields + root helper | 1 |
| 3 | sops backend, generated `.sops.yaml`, auto re-key | 0, 2 |
| 4 | `neo secrets enable` / `disable` | 3 |
| 4b | Config repo remote + push (open question 1) | 0 |
| 5 | `neo restore` | 3, 4b |

Phases 0 and 1 can run in parallel. Each phase is checked with `just launch` and the smoke test.
Phase 4 also gets a VM test: boot a plain machine → enable → verify that no unit restarted → wipe
the VM disk → restore → same host key.

## Decisions

- **sops, latest version, YAML files, `ssh-ed25519` only.** No RSA, no hardware keys, no agenix.
- **Derived values** (bcrypt/htpasswd) are stored as the finished string, as today. No template
  filters.
- **Old generations** with plaintext are left to the automatic GC.
- **Hermes** stays in `neo-git` and uses the identity through the agent.
- **GitHub uses HTTPS tokens** (`core.git.tokens`, encrypted like any secret). The SSH identity is
  only a fallback for `git+ssh://` hosts.

## Open questions

1. **Config repo → GitHub (later):** the machine's config repo has no remote, and neo never
   pushes. Recovery needs `neo.core.configRemote` (HTTPS URL + branch), a push after each commit
   with the `core.git.tokens` entry for that host, and push failures shown in the web UI.
2. **Token expiry:** warn ahead of time (a daily GitHub REST call, which returns the
   `GitHub-Authentication-Token-Expiration` header for PATs), or just surface the push failure?

## Implementation status (2026-10-28)

Done, VM-tested (fresh boot, old→new and new→old switches, `secrets.yaml` switch with byte-identical
rendered files and no restarts):

- `lib.neo.mkSecretOption`, `neo.core.secrets.{templates,containerEnv,backend}`, activation renderer,
  leak guard (`nix/lib/secrets.nix`, `nix/modules/core/secrets.nix`).
- Every secret option in this repo migrated (containers, Hermes, git tokens, tailscale, rathole,
  streamproxy, ical-sync, rustical, docmost, dufs).
- Encrypted values are refused by assertion where a secret is still used at eval: dufs and rustical
  SWAG confs, docmost SSO keys, Hermes `dashboardPassword` (autologin HTML) and `llm.apiKey` for
  provider `custom`.
- `lib.neo.secrets.readYaml` / `load`; the homeserver template merges `secrets.yaml` /
  `secrets.sops.yaml` over `settings.toml` (guarded for template/lib skew).
- CLI: `neo secrets split`; web saves route secret fields into `secrets.yaml` once it exists (and
  refuse changed values while only `secrets.sops.yaml` exists); masked inputs.

Open: phase 0 (identity), phase 3 (sops-nix decryption into `raw/`, recipients, root helper),
`neo secrets enable`, `neo restore`, `secrets.yaml` support in `paste_settings`/upload, and plugin
options (e.g. the arr `apiKey`s in highsea.neo still use plain options and stay inline).
