# Neo Homeserver

<p align="center">
  <img src="cli/static/neo-icon.png" alt="Neo logo" width="160" />
</p>

**Your data. Your machine. Your software.**

Neo is a homeserver operating system. Flash it onto a machine. Easy to use without any technical knowledge, with opinionated defaults and a simple web interface for full control. Reproducible, revertible, recoverable. The easiest way to keep your data private, fully featured so you can free yourself from big tech.

Photos, files, passwords, documents stay on hardware you control.

> If you can't host it and control it, you don't really own it.
>
> [Louis Rossmann](https://www.youtube.com/watch?v=rk3snANxYMY), on repair, ownership, a [self-managed life](https://wiki.futo.org/wiki/Introduction_to_a_Self_Managed_Life:_a_13_hour_%26_28_minute_presentation_by_FUTO_software).

That's Neo's philosophy. Sovereignty over your machine, privacy for your life on it, ownership of the software and the data.

**Documentation.** [Install](docs/INSTALL.md) · [How it works](docs/ARCHITECTURE.md) · [Plugins](docs/PLUGINS.md) · [CLI](docs/CLI.md)

## Getting started

Install Neo. Open the web UI. Set the domain. Set the login. Turn services on. Apply.

Full steps: [Installation guide](docs/INSTALL.md).

## What it looks like

**Every service, one click away.** Over 50 apps across media, files, security, AI and more. Search, filter by category, and see at a glance what's running. The live CPU, RAM and disk bar sits on top.

![Services overview in the Neo web UI](docs/images/services-overview.jpg)

<table>
  <tr>
    <td width="50%"><img src="docs/images/service-immich.jpg" alt="Immich service page: settings grouped into sections" /></td>
    <td width="50%"><img src="docs/images/service-status.jpg" alt="Immich status tab: running units with start, restart, update and logs" /></td>
  </tr>
  <tr>
    <td><b>Configure without YAML.</b> Each service gets a form with plain-language options in tidy sections: where it's reachable from (home network, Tailscale, internet), whether it needs a login, and more.</td>
    <td><b>See what's running.</b> Live status for every container, with start, stop, restart, image pull and logs one click away. Snapshots and clearing app data live right next to it.</td>
  </tr>
</table>

**Never fear a change.** Every save is a version and every activation is a system generation, shown on one timeline. Roll back, boot into an older generation, or restore old settings.

![Versioning timeline: settings versions and system generations](docs/images/versioning.jpg)

<table>
  <tr>
    <td width="68%"><img src="docs/images/what-changed.jpg" alt="What changed: dependency updates and a file diff between two generations" /></td>
    <td width="32%"><img src="docs/images/mobile.jpg" alt="The Neo web UI on a phone" /></td>
  </tr>
  <tr>
    <td><b>Know exactly what changed.</b> Compare any two points in history: updated inputs and plugins with their old and new revisions, plus a full diff.</td>
    <td><b>In your pocket.</b> The whole UI works on a phone.</td>
  </tr>
</table>

## What you need

- A machine you flash Neo onto
- A domain name
- A public IP

If the machine at home has no public IP, route the traffic with [streamproxy](docs/INSTALL.md#no-public-ip-streamproxy). HTTPS still reaches it. The data stays on your hardware.

## Features

### Every day

- **Opinionated defaults.** A firewalled host, services on an internal network, packaging chosen for you.
- **Simple web interface.** Full control in the browser. Enable a service, edit it, preview the changes, apply them, revert an edit before it goes live. Start, stop and restart services, read their logs, restart or shut down the machine.
- **User management.** Add people. Choose the services each person or group may open. Sign in with a password, or with an OAuth provider such as Authentik, Google or Microsoft Entra ID.
- **Automatic HTTPS.** Certificates stay current. Traffic stays encrypted until it reaches your server.
- **Automatic updates.** The operating system, the containers, the apps. Everything, on a schedule, all the way through. A broken container update can be rolled back to the previous image.

### AI support

- **Your IT support.** Hermes helps you run the homeserver. Useful on day one. Useful after you know the system well.
- **Your provider.** The model comes from the AI provider you choose.
- **In every service.** Each service you enable is integrated with Hermes, and Hermes can operate it.
- **Health, reports, recommendations.** Hermes checks the machine, reports what broke, and recommends the next step. Optionally it reviews every update and tells you on Telegram when something went wrong.

### Your hardware

- **Your machine.** The homeserver is hardware you control.
- **Home or anywhere.** Host it at home. Host it anywhere. Safe on a public Wi-Fi uplink.
- **Fine-grained access control.** Per service: reachable on the LAN, on Tailscale, on the public web, or any mix. Per person: which apps they may open.
- **Edge protection.** Allow or block countries and IP ranges. Ban IPs that keep failing to log in, bad bots and exploit scanners (fail2ban, on by default).

### Privacy

- **LAN privacy.** Pi-hole across the network. WireGuard egress for the apps you choose. Tailscale for private reach. NTP for the LAN.
- **Snapshots.** Automatic ZFS snapshots of all Neo data. Restore one service, or everything, from the web UI.
- **Off-site backup.** Scheduled rsync over SSH to a machine you choose.

### Extend

- **Plugins.** Media stacks, personal apps, extra config, on the same HTTPS and the same login. [Plugins](docs/PLUGINS.md).

### Under the hood

- **Nix-based.** This is why the system is reproducible, revertible, recoverable. Rebuild the same machine. Switch or boot back into an earlier generation. Recover from configuration you still hold.

## Services

Enable what you want in the web UI.

- **Files.** Filebrowser, Dufs (WebDAV), Syncthing, Nextcloud, Collabora, Paperless, Docmost (wiki, with single sign-on and an MCP server for AI clients), Stirling PDF, Gitea, Docker registry, off-site backup
- **Media.** Immich, Immich Drop, iSponsorBlockTV. Jellyfin, Sonarr, Radarr and the rest of the \*arr stack come from the [highsea.neo](https://github.com/madebydamo/highsea.neo) plugin
- **Security.** Tinyauth (login in front of every app), Authentik (single sign-on), Vaultwarden (passwords)
- **Calendar and contacts.** RustiCal (CalDAV, CardDAV), Calino, iCal subscriptions
- **Utilities.** Vikunja (tasks), Mealie (recipes), Karakeep (bookmarks), SearXNG (search), pastebin, change detection, Activepieces (automation), Dawarich (location history), Webtop and Firefox (in the browser)
- **Network.** Pi-hole, Tailscale, WireGuard VPN (Gluetun), NTP, Rathole and streamproxy (no public IP)
- **Monitoring.** Beszel
- **AI support.** Hermes
- **Core.** Neo web, SWAG (HTTPS and routing), system and container updaters
- **Your own.** Neo is fully extendable with your own Nix plugins. Whatever you can imagine. [Plugins](docs/PLUGINS.md).

## For contributors

Work on Neo itself starts at **[AGENTS.md](AGENTS.md)**.

---

**Own your stack.** Issues welcome. Contributions welcome.
