# Neo Homeserver

A fully declarative, self-replicating NixOS homeserver built with flakes, OCI containers, and the `neo` CLI.

This template gives you a complete homeserver with automatic updates, reverse proxy (SWAG), authentication, backup, and many self-hosted services.

## Prerequisites

**Step 1: Install NixOS**

1. Install a **minimal NixOS** installation (physical machine or VM).
2. Make sure you have:
   - SSH access with your public key
   - Internet connection
   - At least 2 CPU cores and 4GB RAM recommended (for multiple services)

No additional configuration is needed during NixOS install — the `neo` CLI will handle everything.

## Quick Start

### 1. Configure your services

Create a `./settings.toml` on your computer with your domain, email, API keys, and which services you want enabled.

See the example below.

### 2. Initialize the configuration

On your fresh NixOS machine, run:

```bash
nix-shell --extra-experimental-features "nix-command flakes" -p git # not needed if git is available
nix run --extra-experimental-features "nix-command flakes" --refresh github:madebydamo/neo#neo -- init
```

This creates a new git repository with all necessary files.

### 3. Deploy

```bash
nix run --extra-experimental-features "nix-command flakes" github:madebydamo/neo#neo -- activate
```

This will:

- Build your system
- Switch to the new configuration
- Start all enabled services (via Docker/OCI containers)

After the first activation, you can use the neo cli for simple management.
Your configuration will by default be found at `/var/neo/DATA/AppData/configuration`.
To edit your configuration, make desired changes at `/var/neo/DATA/AppData/configuration/settings.toml` and run `neo activate`.

```bash
neo --help
vi /var/neo/DATA/AppData/configuration/settings.toml # do desired edits.
neo activate # changes will apply, on errors you see a message.
```

## Example `settings.toml`

```toml
[nixos]
enabled = true
bootstrapEnabled = true
autoUpdateEnabled = true

[ssh]
authorizedKeys = ["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI..."]

[services.swag]
enabled = true
domain = "example.com"
email = "you@example.com"

[services.tinyauth]
enabled = true
users = ["username:$2a$10$..."]  # Use `docker run -i -t --rm ghcr.io/steveiliop56/tinyauth:v5 user create --interactive` to generate

[services.rathole]
enabled = false # only useful if you have just one server with public ip. That one should run services.streamproxy

[services.filebrowser]
enabled = true

[services.backup]
enabled = true
remotePath = "yourservername"
remoteServer = "zh1234.rsync.net"
remoteUser = "zh1234"

[services.tailscale]
enabled = true
authKey = "tskey-..."
acceptRoutes = true
advertiseExitNode = true

[services.paperless]
enabled = true

[services.changedetection]
enabled = true

[services.pastebin]
enabled = true

[services.immich]
enabled = true

[services."immich-drop"]
enabled = true

[services.hermes]
enabled = true
telegramBotToken = "3473438487:AKRfdaei..." # Create a bot by sending @botfather /newbot
telegramAllowedUserId = [825821185] # whitelist your own id
xaiApiKey = "xai-DdjfkKJfIO..."
defaultModel = "xai/grok-4.20-0309-reasoning"

```

## Available Services

- **swag**: Nginx reverse proxy + Let's Encrypt
- **tinyauth**: Lightweight auth gateway
- **filebrowser**: Web file manager
- **immich**: Photo and video backup
- **paperless**: Document management
- **changedetection**: Website change monitoring
- **hermes**: AI assistant with Telegram integration
- **rathole**: Secure tunnel (for exposing services)
- **tailscale**: Zero-config VPN
- **backup**: Automated rsync backups
- And more (nextcloud, vaultwarden, pihole, etc.)

## Management Commands

Once deployed, use these commands:

```bash
neo init           # initialized nixos configurations with your settings properly
neo update         # updates all dependencies
neo build          # builds your current configuration
neo activate       # activates your configuration
neo nuke           # nukes the configuration, can be recovered with a fresh init
```

See `neo help` for all commands.

## Updating

The system auto-updates daily. To update manually:

```bash
neo update && neo activate
```

For more help, see the [neo repository](https://github.com/madebydamo/neo).
Issues on Problems are appreciated.

---

**Made with ❤️ using Nix + Rust**
