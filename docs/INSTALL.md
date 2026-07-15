# Installation

Neo aims to be usable without a sysadmin background. **After setup, you mostly use the web UI.** This page covers getting the machine installed once; the command snippets below are the bootstrap steps—you will not live in the terminal day to day.

Two paths:

| Path | When to use |
|------|-------------|
| **[A — Machine already runs NixOS](#path-a--machine-already-runs-nixos)** | Fresh or existing NixOS host you want to turn into Neo |
| **[B — Install from your laptop](#path-b--install-from-your-laptop-nixos-anywhere)** | Any suitable Linux (or installer) over SSH; prepare config locally, install remotely |

Also: [what you need](#what-you-need), [minimal starter config](#minimal-starter-config), [disks (Disko)](#automatic-disk-layout-disko), [no public IP](#no-public-ip-streamproxy), [after install](#after-install), [learning links](#learning-links).

---

## What you need

**In plain terms:**

- A computer (or VPS) for the homeserver  
- A **domain name**  
- Either a **public IP** with ports **80 and 443** reachable, **or** access to a **streamproxy** on a public machine (see [below](#no-public-ip-streamproxy))  
- SSH access during install  

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
- Network path to the target over SSH  

**Target machine** ([nixos-anywhere requirements](https://nix-community.github.io/nixos-anywhere/)):

- Reachable via **SSH** as root, or a user with **passwordless sudo**  
- Either a **NixOS installer**, or **x86_64 / aarch64 Linux with [kexec](https://man7.org/linux/man-pages/man8/kexec.8.html)** support (common on VPS images), and roughly **≥ 1.5 GB RAM** free for the installer step  
- You accept that Disko will **erase** the disks you configure  

Official docs require **kexec** (or a NixOS installer)—not KVM—on the target for the remote install path.

### 1. Init Neo on the laptop

```bash
mkdir -p ~/neo-homeserver && cd ~/neo-homeserver

cat > settings.toml << 'EOF'
[neo-cli]
template = "github:madebydamo/neo#homeserver"
bootstrapMethod = "template"

# Optional: local defaults to ./build, server defaults to /var/neo/DATA/AppData/configuration
# [neo-cli.local]
# configPath = "./build"
# [neo-cli.server]
# configPath = "/var/neo/DATA/AppData/configuration"

[services.system-updater]
enabled = true
EOF

nix run --extra-experimental-features "nix-command flakes" --refresh \
  github:madebydamo/neo#neo -- --settings ./settings.toml init
```

Configuration appears under `./build` (local profile). The same settings file carries a **server** profile path for the homeserver and system-updater — no manual path rewrite before install.

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

### 5. First login

```bash
ssh root@TARGET_IP
```

Open the **Neo web UI** in a browser (once DNS and SWAG are happy) and manage the homeserver from there.

---

## Minimal starter config

Enough for a machine with **public IP**, **DNS** for your domain, and ports **80/443** open.

Generate a tinyauth password hash:

```bash
docker run -i -t --rm ghcr.io/steveiliop56/tinyauth:v5 user create --interactive
```

(or use helpers in Neo web after bootstrap)

```toml
[core]
hostname = "homeserver"

[core.ssh]
authorizedKeys = [
  "ssh-ed25519 AAAA... your-key",
]

[neo-cli]
template = "github:madebydamo/neo#homeserver"
# local.configPath defaults to ./build; server.configPath defaults to appdata/configuration

[services.system-updater]
enabled = true

[services.swag]
enabled = true
domain = "example.com"
email = "you@example.com"

[services.tinyauth]
enabled = true
users = [
  "admin:$2a$10$REPLACE_WITH_BCRYPT_HASH",
]

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

---

## Automatic disk layout (Disko)

For greenfield installs, Neo can partition with [Disko](https://github.com/nix-community/disko):

| Setting | Meaning |
|---------|---------|
| `enabled` | Turn on automatic layout |
| `mainDisk` | Disk to wipe for the OS (EFI + ZFS) |
| `poolName` | ZFS pool name (default `zroot`) |
| `additionalDisks` | Extra disks → mount points (e.g. media) |

Default: boot partition + ZFS for the system and Neo data (with snapshots on Neo’s data dataset).

**Only enable when you intend to erase those disks.**

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
port = 2223
name = "home"
```

One public IP and one homeserver? Run SWAG on that IP and leave streamproxy off.

---

## After install

1. Open **Neo web**  
2. Enable services, set users, add [plugins](PLUGINS.md) under **Settings → core → plugins**  
3. **Activate** from the UI  
4. Leave **system-updater** (and **docker-updater**) on if you want hands-off upgrades  

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
