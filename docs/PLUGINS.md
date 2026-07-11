# Plugins

Plugins let you **extend Neo** without forking the whole project. They are packages of extra Nix code that plug into your homeserver: more apps, personal projects, disks, system tweaks—while still using Neo’s HTTPS, auth, and updates.

You do **not** need to understand how Neo wires plugins internally. In daily use you only add a plugin URL in the UI and turn on the services it provides.

## What plugins are for

| Use case | Example |
|----------|---------|
| Bigger app stacks Neo doesn’t ship in core | Media / *arr automation |
| Personal apps you want behind the same HTTPS + login | A small site or tool of your own |
| Extra system bits | Additional disks, packages, one-off Nix modules |

Example of a personal service without fighting TLS yourself: **[portrait](https://github.com/madebydamo/portrait)**—host your own thing on the same secure front door as the rest of the homeserver.

## Awesome plugins

Community and official extensions worth knowing:

| Plugin | What it adds |
|--------|----------------|
| **[highsea.neo](https://github.com/madebydamo/highsea.neo)** | Media server stack: Jellyfin, Sonarr, Radarr, Prowlarr, qBittorrent, Bazarr, Tdarr, Seerr, FlareSolverr, helpers—full *arr-style automation on Neo |
| **[portrait](https://github.com/madebydamo/portrait)** | Personal / custom hosting without redoing certificates and reverse proxy from scratch |

Know a great plugin? Open an issue or PR on Neo to get it listed.

## Install a plugin (web UI)

1. Open **Neo web**.
2. Go to **Settings → neo-service → plugins**.
3. Add a plugin entry (remote or local—examples below).
4. Apply / activate when the UI asks you to (so the new apps appear).
5. Enable the services you want under **Services** (same place as built-in apps).

That’s it for normal use. No need to edit files by hand unless you prefer to.

### Remote plugin (recommended)

Use a public flake URL:

```text
github:madebydamo/highsea.neo
```

### Local plugin (development or private code)

If the plugin lives on disk:

```text
git+file:/home/you/projects/high_sea
```

or:

```text
path:/home/you/projects/my-plugin
```

You can mix several plugins. Order usually doesn’t matter for everyday use.

## Create your own plugin

If you want to package services or Nix config for yourself or others:

1. Start from Neo’s plugin template:

   ```bash
   mkdir my-plugin && cd my-plugin
   nix flake init -t github:madebydamo/neo#plugin
   ```

2. Replace the example service under `modules/services/` with your app (options, container or systemd unit, optional reverse-proxy snippet)—same patterns as core services in this repo.
3. Publish the flake (GitHub, etc.) or keep it on a path and add it under **Settings → neo-service → plugins**.

Developers implementing services: see [AGENTS.md](../AGENTS.md) and existing modules under `nix/services/`. Users of Neo do not need that level of detail.

## After installing

- New options show up in the web UI once the plugin is applied.
- HTTPS and login gates stay consistent with the rest of Neo when the plugin follows Neo’s reverse-proxy helpers.
- Updates follow the same auto-update story as the rest of the homeserver.

## See also

- [Installation](INSTALL.md) — first-time setup  
- [How it works](ARCHITECTURE.md) — security and traffic model  
- [README](../README.md) — product overview  
