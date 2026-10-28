# Installation

Neo aims to be usable without a sysadmin background. **After setup, you mostly use the web UI.** This page covers getting the machine installed once; the command snippets below are the bootstrap steps—you will not live in the terminal day to day.

Two paths, a fallback when the laptop cannot build the system, and the way back after a dead machine:

| Path | When to use |
|------|-------------|
| **[A — Machine already runs NixOS](#path-a--machine-already-runs-nixos)** | Fresh or existing NixOS host you want to turn into Neo |
| **[B — Install from your laptop](#path-b--install-from-your-laptop-nixos-anywhere)** | Linux laptop that can build `x86_64-linux`; prepare config locally, install remotely with nixos-anywhere |
| **[Laptop cannot build](#when-the-laptop-cannot-build-mac-live-usb-or-wrong-architecture)** | Mac, architecture mismatch, or a live USB with too little disk — build **on the target** |
| **[Rebuild from the config repo](#rebuild-a-machine-from-its-config-repo)** | A machine died: reinstall a new one from its config repo with `neo restore` |

Also: [what you need](#what-you-need), [first login](#first-login), [minimal starter config](#minimal-starter-config), [sign-in with OAuth](#sign-in-with-oauth-optional), [disks (Disko)](#automatic-disk-layout-disko), [no public IP](#no-public-ip-streamproxy), [per-service ingress](#per-service-ingress), [secrets and encryption](#secrets-and-encryption), [after install](#after-install), [learning links](#learning-links).

---

## What you need

**In plain terms:**

- A computer (or VPS) for the homeserver  
- A **domain name**  
- Either a **public IP** with ports **80 and 443** reachable, **or** access to a **streamproxy** on a public machine (see [below](#no-public-ip-streamproxy))  
- SSH access during install, using **your** SSH public key in `core.ssh.authorizedKeys`  

There is **no default Linux password**. After install, SSH as `homeserver@…` (for `neo`) or `admin@…` with the key you put in settings. Both users share `core.hashedLinuxPassword` when you set one (console/sudo).

**You do not need:** to learn Docker Compose, hand-write Nginx configs, or open lots of ports. Only web traffic (80/443) is required from the outside.

---

## Path A — Machine already runs NixOS

### Prerequisites

- Minimal **NixOS** with network and **SSH** (public key preferred)  
- About **2+ cores and 4+ GB RAM** for several services (more for heavy media)  
- Domain DNS pointing at this host’s public IP (if not using streamproxy)  

Disk partitioning is already done on a normal install. Automatic wipe/format ([Disko](#automatic-disk-layout-disko)) is for Path B / reinstalls—not for a machine full of data you care about.

### 1. Bootstrap Neo on the host

```bash
nix-shell --extra-experimental-features "nix-command flakes" -p git

nix run --extra-experimental-features "nix-command flakes" --refresh \
  github:madebydamo/neo#neo -- init
```

This creates your homeserver configuration (from the official template) and starter settings.

After a full Neo install, configuration usually lives under:

`/var/neo/DATA/AppData/configuration`

### 2. Configure (prefer the web UI when available)

For the **first** bring-up, put at least the [minimal starter config](#minimal-starter-config) in place (domain, email, SSH key, login user, SWAG + Neo UI enabled).

Then apply:

```bash
nix run --extra-experimental-features "nix-command flakes" \
  github:madebydamo/neo#neo -- activate
```

Once the **Neo** service is up, open the web UI in your browser. From then on:

1. Change options in the UI  
2. Use **Activate** in the UI  

You should not need the terminal for everyday service toggles.

### 3. Day-to-day (normal users)

| Do this | Not required |
|---------|----------------|
| Neo web → change settings → Activate | Learning the full CLI |
| Keep auto-update enabled if you want hands-off upgrades | Manual package hunts |

Power users: optional [CLI reference](CLI.md).

---

## Path B — Install from your laptop (nixos-anywhere)

Use this when the target is **not** Neo yet, or you want a clean install from another computer. Can **format the remote disk** automatically ([Disko](#automatic-disk-layout-disko)).

### Prerequisites

**Your laptop:**

- [Nix installed](https://nixos.org/download/) with [flakes enabled](https://wiki.nixos.org/wiki/Flakes)  
- Must be able to **build `x86_64-linux`** (a typical x86_64 Linux machine). `nix run github:madebydamo/neo#neo` currently only provides that architecture.  
- Network path to the target over SSH  

If you are on a **Mac**, a live USB, or a machine that cannot build `x86_64-linux`, skip this path and use [when the laptop cannot build](#when-the-laptop-cannot-build-mac-live-usb-or-wrong-architecture). Passing `--system x86_64-linux` on Darwin still needs a Linux builder; it will not magically cross-compile NixOS.

**Target machine** ([nixos-anywhere requirements](https://nix-community.github.io/nixos-anywhere/)):

- Reachable via **SSH** as root, or a user with **passwordless sudo**  
- Either a **NixOS installer**, or **x86_64 / aarch64 Linux with [kexec](https://man7.org/linux/man-pages/man8/kexec.8.html)** support (common on VPS images), and roughly **≥ 1.5 GB RAM** free for the installer step  
- You accept that Disko will **erase** the disks you configure  

Official docs require **kexec** (or a NixOS installer)—not KVM—on the target for the remote install path. A live USB’s root is usually a **tmpfs**; there is often not enough space to build on the target unless you [mount extra disk](#when-the-laptop-cannot-build-mac-live-usb-or-wrong-architecture).

### 1. Init Neo on the laptop

```bash
mkdir -p ~/neo-homeserver && cd ~/neo-homeserver

cat > settings.toml << 'EOF'
# neoInput defaults to github:madebydamo/neo on both profiles.
# Put a laptop checkout on local only; the server profile stays on GitHub.
# [neo-cli.local]
# configPath = "./build"
# neoInput = "git+file:/path/to/neo"
# template = "/path/to/neo#homeserver"
# [neo-cli.server]
# configPath = "/var/neo/DATA/AppData/configuration"
# neoInput = "github:madebydamo/neo"

[services.system-updater]
enabled = true
EOF

nix run --extra-experimental-features "nix-command flakes" --refresh \
  github:madebydamo/neo#neo -- --settings ./settings.toml init
```

Configuration appears under `./build` (local profile). The same settings file carries a **server** profile path for the homeserver and neo-bootstrap — no manual path rewrite before install.

### 2. Fill in domain, keys, disks

Before installing, set at least:

1. **Your SSH public key** (or you lock yourself out)  
2. **Disko** enabled and `mainDisk` set to the target disk (`lsblk` over SSH—e.g. `/dev/sda`, `/dev/vda`, `/dev/nvme0n1`)  
3. Domain, email, tinyauth user, services — [minimal starter config](#minimal-starter-config)  

### 3. Edit with Neo web on the laptop (do **not** Activate for the remote)

```bash
nix run --extra-experimental-features "nix-command flakes" \
  github:madebydamo/neo#neo -- --settings ./settings.toml web
```

Use the UI to fill domain, keys, Disko disk path, users, plugins, etc.

**Important:** do **not** press **Activate** here if this config is meant for the **remote** machine. Activate applies to the computer you are sitting on. For Path B, first install is **nixos-anywhere**; Activate is for later, **on the server**.

### 4. Install with nixos-anywhere

```bash
cd build
nix flake show   # confirm nixosConfigurations.neo exists

nix run github:nix-community/nixos-anywhere -- \
  --flake .#neo \
  --generate-hardware-config nixos-generate-config ./hardware-configuration.nix \
  --target-host root@TARGET_IP
```

With Disko enabled in your config, Neo already describes the disk layout—no separate disk file required for the default single-disk ZFS setup.

When it finishes, the target reboots into Neo/NixOS. SSH host keys change; clear old entries if needed:

```bash
ssh-keygen -R TARGET_IP
```

Then [first login](#first-login) as `homeserver@TARGET_IP`. Root SSH is disabled (`PermitRootLogin = no`).

---

## When the laptop cannot build (Mac, live USB, or wrong architecture)

Path B fails when:

- The laptop is **not** `x86_64-linux` (typical error on an Apple Silicon Mac: flake does not provide `apps.aarch64-darwin.neo` / `packages.aarch64-darwin.neo`)
- The laptop and target have **different architectures**, so nixos-anywhere cannot build the system locally
- The target is a **NixOS live USB** (or similar): `/` and `/nix` sit on a tmpfs, so a local `neo` / NixOS build runs out of space

**Workaround:** treat the target as Path A. Boot it (installer USB is fine), give Nix a real disk for the build, run `neo init` **on the machine**, then let Disko format the OS disk. The target still needs to be **x86_64-linux** for `github:madebydamo/neo#neo` (same CLI limitation as Path B).

If the box has **two drives** (example: SSD for Neo, HDD for extra space):

1. In Disko, set `mainDisk` to the **SSD** you are willing to wipe (the OS disk).
2. On the live system, **mount the HDD** somewhere durable and point Nix scratch at it so the build is not stuck in RAM/tmpfs. Device names vary (`lsblk`); do **not** mount the disk Disko will erase.

```bash
# Example only — use the extra disk, not mainDisk
lsblk
mkdir -p /mnt/scratch
mount /dev/sdb1 /mnt/scratch          # extra HDD partition
mkdir -p /mnt/scratch/tmp /mnt/scratch/nix
export TMPDIR=/mnt/scratch/tmp
# Optional: if /nix is still filling the USB tmpfs:
#   mount --bind /mnt/scratch/nix /nix
```

3. Install Nix if needed, then run the [Path A](#path-a--machine-already-runs-nixos) `nix run github:madebydamo/neo#neo -- init` commands **on the target** (not from the Mac).
4. Put [minimal starter config](#minimal-starter-config) in place, including your SSH public key and Disko `mainDisk`.
5. `neo activate` (or the equivalent first switch) on the target. Disko formats the SSD.
6. After Neo is up, you can add the HDD under Disko `additionalDisks` in the web UI **if you accept that Disko will wipe that disk**. Unmount the scratch mount first. If you want to keep the HDD as-is, leave it out of Disko.

`nixos-anywhere --build-on-remote` is the same idea (build on the target) and has the same space requirement: the live image still needs a disk for `/nix` / `TMPDIR`.

---

## First login

After install, SSH as **`homeserver`** with the key in `core.ssh.authorizedKeys`:

```bash
ssh homeserver@TARGET_IP
```

- There is **no default password**.
- Root login over SSH is off.
- `homeserver` is the account `neo` and the web UI run as. SSH there and `neo edit` / `neo web` work without a sudo password.
- `admin` is a separate wheel user (same SSH keys, same `hashedLinuxPassword`) for interactive sudo and remote Nix builds. `neo` as `admin` re-execs with `sudo -u homeserver`, which needs that password.
- Optional: set `core.hashedLinuxPassword` (web UI helper or `mkpasswd -m sha-512`) if you want a console or sudo password. Leave it empty to keep password login locked.

Open the **Neo web UI** in a browser (once DNS and SWAG are happy) and manage the homeserver from there.

---

## Minimal starter config

Enough for a machine with **public IP**, **DNS** for your domain, and ports **80/443** open.

Generate a tinyauth password hash:

```bash
docker run -i -t --rm ghcr.io/tinyauthapp/tinyauth:v5.2.0 user create --interactive
```

(or use helpers in Neo web after bootstrap)

```toml
[core]
hostname = "homeserver"
# Optional. Empty = no Linux password (SSH keys only). There is no default.
# hashedLinuxPassword = "$6$rounds=...hash-from-mkpasswd..."

[core.ssh]
authorizedKeys = [
  "ssh-ed25519 AAAA... your-key",
]

# [neo-cli]
# local and server neoInput default to github:madebydamo/neo
# local.configPath defaults to ./build; server.configPath defaults to appdata/configuration

[services.system-updater]
enabled = true

[services.swag]
enabled = true
domain = "example.com"
email = "you@example.com"

[services.tinyauth]
enabled = true
# Web UI login (not a Linux account). Linux SSH is homeserver@ + authorizedKeys.
users = [
  "you:$2a$10$REPLACE_WITH_BCRYPT_HASH",
]
# Optional per-user app ACLs (neo service names). Empty = full access.
# [services.tinyauth.access.guest]
# allow = ["searxng", "pastebin"]
# [services.tinyauth.access.kid]
# block = ["webtop"]

[services.neo]
enabled = true

[services.filebrowser]
enabled = true
```

**Path B — add Disko** (destructive format of that disk):

```toml
[disko]
enabled = true
mainDisk = "/dev/sda"
```

### Sign in with OAuth (optional)

Tinyauth can offer one or more OAuth sign-in buttons next to (or instead of) the local `users`. You need at least one user or one enabled provider. Each provider has an ID (lowercase letters and digits). Register this redirect URI at the provider: `https://<tinyauth subdomain>.<domain>/api/oauth/callback/<id>`.

| Type | Use for |
|------|---------|
| `generic` | Any OAuth2 / OIDC server: Authentik on this box, Keycloak, Google, … Set `authUrl`, `tokenUrl`, `userinfoUrl`. |
| `entra` | Microsoft Entra ID. Set `tenantId`; URLs and groups come from Microsoft. |

```toml
[services.tinyauth.oauth.authentik]
type = "generic"
buttonName = "Authentik"
clientId = "..."
clientSecret = "..."
authUrl = "https://authentik.example.com/application/o/authorize/"
tokenUrl = "https://authentik.example.com/application/o/token/"
userinfoUrl = "https://authentik.example.com/application/o/userinfo/"
```

`oauthGroups` lists the groups whose members may use apps at all; `groupAccess` narrows each group to some apps. `oauthAutoRedirect` skips the login page and goes straight to one provider. In the web UI all of this sits in the tinyauth OAuth section.

Authentik itself is an optional service (`[services.authentik]`). It is the login for its own site, so tinyauth does not sit in front of it. Fill `services.authentik.email` (SMTP) if you want password recovery and magic-link mails.

---

## Automatic disk layout (Disko)

For greenfield installs, Neo can partition with [Disko](https://github.com/nix-community/disko):

| Setting | Meaning |
|---------|---------|
| `enabled` | Turn on automatic layout |
| `mainDisk` | Disk to wipe for the OS (EFI + ZFS) |
| `poolName` | ZFS pool name (default `zroot`) |
| `additionalDisks` | Extra disks → mount points (e.g. media) |
| `autoSnapshot` | Automatic snapshots of Neo data and how many to keep (default: 4 × 15-minute, 12 hourly, 7 daily, 4 weekly, 3 monthly) |

Default: boot partition + ZFS for the system and Neo data (with snapshots on Neo’s data dataset). Snapshots you take by hand in the web UI are never pruned, and pinned ones are kept. One disk today: `mainDisk` becomes the OS pool (`zroot`). Extra disks in `additionalDisks` each get their own pool.

Snapshots are restorable from the web UI (only Neo's data dataset is snapshotted, never the system or `/nix`):

- **Service pane → Snapshots:** list, *Snapshot now*, *Restore*. Restore takes a `neo-prerestore-<service>-…` snapshot, stops `neo-<service>.target`, copies the service's appdata folder back from the snapshot (other services are untouched), and starts it again.
- **Versioning → Data snapshots:** restores *all* Neo data (every service's appdata and the config repo). The server reboots and the initrd swaps `zroot/neo` for a clone of the snapshot; the replaced state stays as `zroot/neo.prev-<ts>` and is listed there, so a restore can be undone. This restores files only. Run *Activate* afterwards, or use *+ boot gen N* to also boot the system generation that was active when the snapshot was taken.

**Only enable when you intend to erase those disks.**

Live USB with too little space: [mount a second disk for the build](#when-the-laptop-cannot-build-mac-live-usb-or-wrong-architecture), keep `mainDisk` on the drive you want Disko to format.

---

## No public IP? Streamproxy

If the homeserver cannot accept inbound 80/443:

| Role | Where | Job |
|------|--------|-----|
| **Public edge** | VPS / box with public IP | **streamproxy** (+ optional local apps) |
| **Homeserver** | Home / private network | Your **data and apps**; tunnels out |

You still need:

1. **DNS** for your domains → **public** machine’s IP  
2. A working tunnel (rathole client on the homeserver, matching token on streamproxy)  
3. HTTPS and auth still terminate in the Neo stack as described in [ARCHITECTURE.md](ARCHITECTURE.md)  

Illustrative fragments (tokens must match; check current ports in the UI):

**Public edge:**

```toml
[services.streamproxy]
enabled = true

[services.streamproxy.entries.home]
url = "home.example.com"
token = "shared-secret-token"
wildcard = true
includeTopLevel = true
```

**Homeserver (data stays here):**

```toml
[services.swag]
enabled = true
domain = "home.example.com"
email = "you@example.com"

[services.rathole]
enabled = true
token = "shared-secret-token"
remoteAddr = "edge.example.com"
name = "home"
```

`port` defaults to 2223, the streamproxy tunnel port. Change it only if your edge uses another port. `name` is the streamproxy entry name (`home` above).

One public IP and one homeserver? Run SWAG on that IP and leave streamproxy off.

### Per-service ingress

Each reverse-proxied service has an **`ingress`** multi-select: **`local`** (LAN), **`tailscale`** (tailnet), **`web`** (public HTTPS via rathole/streamproxy). Pick any combination; the default is all three (same behaviour as before). Dropping **`web`** makes that vhost return the neo **404** page on the public PROXY-protocol path; HTTP-01 on port 80 still works so certificates renew. Example — pastebin LAN-only:

```toml
[services.pastebin]
enabled = true
ingress = ["local"]
```

## Secrets and encryption

Passwords, tokens and API keys (settings marked as secret, shown masked in the web UI) can live in three places. Pick one under **Settings → Secrets**:

| Where | What it means |
|-------|---------------|
| **In settings.toml** (default) | As before: everything in one file. Keep the config repo private. |
| **secrets.yaml** (*Move to secrets.yaml*) | Secrets in their own file next to `settings.toml`, still plain text. |
| **Encrypted** (*Encrypt*) | Secrets in `secrets.sops.yaml`, encrypted with [sops](https://github.com/getsops/sops). Safe to keep in a repo on GitHub. |

Editing doesn't change: the web UI shows and saves secrets the same way in all three modes. **Activate** after switching.

**Who can decrypt.** The machine itself (its SSH host key, `/etc/ssh/ssh_host_ed25519_key`) and every `ssh-ed25519` key in **System → SSH** (`core.ssh.authorizedKeys`). If you can log in with an ed25519 key, you can decrypt. RSA and other key types can still log in but can't decrypt. Encrypting refuses to start without at least one ed25519 key.

**Escrow.** When you encrypt on the machine, Neo also writes `identity.sops.yaml`: the machine's SSH host key, encrypted to your SSH keys only (not to the machine). With it, a reinstalled machine gets its old identity back and can decrypt its secrets again ([rebuild from the config repo](#rebuild-a-machine-from-its-config-repo)).

**Adding or removing SSH keys.** A save in the web UI re-encrypts right away. After editing `authorizedKeys` by hand, run `neo secrets rekey`. A removed key can still read old versions in the git history: change the secrets themselves if a key is compromised.

**Terminal.** `neo secrets split | enable | disable | rekey` do the same as the buttons ([CLI](CLI.md)). On a checkout with your own key, `sops edit secrets.sops.yaml` works too (`.sops.yaml` is generated). sops reads your key from `~/.ssh/id_ed25519`, or from `SOPS_AGE_SSH_PRIVATE_KEY_FILE`.

What stays out of git anyway: decrypted values only exist in `/run/neo-secrets` (RAM) on the running machine, readable by root and the service that needs them.

## Rebuild a machine from its config repo

If the machine dies, a new one can install itself from the config repo. You need:

- the config repo on a git host. Neo doesn't push it yet. Push it yourself, as `homeserver` on the machine:

  ```bash
  cd /var/neo/DATA/AppData/configuration     # neo-cli.server.configPath
  git remote add origin git@github.com:you/homeserver-config.git
  git push -u origin HEAD
  ```

  Encrypt first ([above](#secrets-and-encryption)) if the repo isn't private.
- one of the ed25519 private keys listed in **System → SSH** at the time of the last push.
- your data from backups (snapshots and the **backup** service). The repo holds configuration, not appdata.

**Steps** (on the new machine, booted from a NixOS installer, as root):

1. Partition and mount the target at `/mnt`. With Neo's Disko layout, Disko can do it from the repo (check `mainDisk` first, it is wiped):

   ```bash
   git clone git@github.com:you/homeserver-config.git /tmp/config
   nix --extra-experimental-features "nix-command flakes" run github:nix-community/disko/latest -- \
     --mode destroy,format,mount --flake /tmp/config#neo
   ```

   Without Disko, partition by hand and adjust `hardware-configuration.nix` in the repo.
2. Restore and install:

   ```bash
   nix --extra-experimental-features "nix-command flakes" run github:madebydamo/neo#neo -- \
     restore git@github.com:you/homeserver-config.git --key ~/.ssh/id_ed25519
   ```

   This clones the repo to `/mnt/<configPath>`, decrypts `identity.sops.yaml` with your key and writes the old SSH host key to `/mnt/etc/ssh`. It refuses a key that doesn't match `core.secrets.hostKey` in the repo. It then hands `core.git.tokens` to Nix (for private plugins) and runs `nixos-install`. `--no-install` stops after the clone and the host key, `--target` changes `/mnt`, `--config` the configuration name (default `neo`).
3. Reboot. The machine has its old SSH host key, so clients don't warn about it, and it decrypts its secrets on first boot. Restore appdata from your backups, then **Activate**.

For a private repo, use an SSH URL with your key loaded (`ssh-agent`, or `ssh -A` into the installer). Don't put a token in the URL: it would end up as the restored repo's remote.

Without `identity.sops.yaml` (plain secrets, or a repo that was never encrypted on the machine), the restore still works, but the machine gets a new host key. Re-trust it on your clients, and update forges if `core.git.useHostKey` is on. An encrypted repo without an escrow can't be restored this way: decrypt (`neo secrets disable`) and re-encrypt on a running machine first.

---

## After install

1. SSH as `homeserver@…` if you need a shell ([first login](#first-login))  
2. Open **Neo web**  
3. Enable services, set users, add [plugins](PLUGINS.md) under **Settings → System → Plugins**  
4. **Activate** from the UI. The changes preview shows what will change first, including updated flake inputs and plugins  
5. Leave **system-updater** (and **docker-updater**) on if you want hands-off upgrades  
6. Optional: [encrypt secrets](#secrets-and-encryption) and push the config repo, so the machine can be [rebuilt from it](#rebuild-a-machine-from-its-config-repo)
7. Optional: set up [backups](#automatic-disk-layout-disko) (ZFS snapshots, the **backup** service for off-site rsync), and the SWAG **Geo & IP filter** and **Auto ban** sections if the machine is on the public web  

Restart and shut down live under **Settings → Power**. Booting an older generation lives under **Versioning**.

Optional terminal tools: [CLI.md](CLI.md).

---

## Learning links

| Topic | Link |
|-------|------|
| Install Nix | https://nixos.org/download/ |
| Flakes (if Path B) | https://wiki.nixos.org/wiki/Flakes |
| nixos-anywhere | https://nix-community.github.io/nixos-anywhere/ |
| Disko | https://github.com/nix-community/disko |
| Ownership / self-managed life (inspiration) | [Louis Rossmann · decline of ownership](https://www.youtube.com/watch?v=rk3snANxYMY) · [FUTO self-managed guide](https://wiki.futo.org/wiki/Introduction_to_a_Self_Managed_Life:_a_13_hour_%26_28_minute_presentation_by_FUTO_software) |
| How Neo protects traffic | [ARCHITECTURE.md](ARCHITECTURE.md) |

Questions and issues: [github.com/madebydamo/neo](https://github.com/madebydamo/neo).
