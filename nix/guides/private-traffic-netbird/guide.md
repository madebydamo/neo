Away from home, your phone normally reaches an app like `https://photos.{{domain}}`
over the public internet. With [NetBird](service:netbird) on the server and on
your devices, they talk to the server **directly**, over a private, encrypted
network that only your own devices can join. The addresses and HTTPS
certificates stay the same, so nothing changes for the apps.

The network can run on one of your own servers with
[NetBird server](service:netbird-server), so no outside account is needed. Every
other Neo server can join it too. Once that works, you can close apps to the
internet completely and still use them from anywhere.

## What you need

- **Your apps already open at `https://<app>.{{domain}}`.** HTTPS certificates
  still come from the web gateway ([SWAG](service:swag)), and to renew them the
  server must stay reachable from the internet on port 80: a public IP address
  with port 80 forwarded, or a tunnel server with [Rathole](service:rathole).
- A NetBird network: your own, run by [NetBird server](service:netbird-server)
  on a server with a public address, or an account at netbird.io.
- The NetBird app on the phones and laptops you want to use.

## 1. Get a setup key

Open the NetBird dashboard of your network and go to **Setup Keys**. Create a
key for this server. A **one-off** key is enough; a **reusable** one can add
several servers.

> [!tip]
> If this server runs [NetBird server](service:netbird-server) itself, skip
> this step: it adds itself to its own network.

## 2. Connect the server

Open [NetBird](service:netbird) and switch it on. Fill in:

- [Setup key](option:netbird.setupKey): the key from step 1.
- [Network address](option:netbird.managementUrl): the address of your network,
  for example `https://netbird.example.com`. Leave it empty for netbird.io or
  for NetBird server on this machine.
- [Network address range](option:netbird.networkRange): the range shown in the
  dashboard under **Settings → Networks**. A network run by Neo uses
  `10.213.0.0/16`, the default.

Apply the change. The server appears in the dashboard under **Peers**. Note its
NetBird address.

> [!warning]
> Tailscale on as well? Only one of them can manage this server's DNS: turn off
> [Use Tailscale DNS](option:tailscale.acceptDns) or
> [Use NetBird DNS](option:netbird.acceptDns). And the NetBird network must not
> use addresses starting with `100.` (Neo refuses the change), because
> Tailscale's firewall would block it.

## 3. Let your devices find the apps

In the dashboard open **DNS → Zones** and add a zone:

- **Domain:** `{{domain}}`, with the group **All**.
- Two records of type **A**, both pointing at this server's NetBird address:
  `*` (every app) and `@` (the domain itself).

Then turn on [App names over NetBird](option:netbird.appNames) and apply.
Every device in your network now finds `photos.{{domain}}` at this server's
NetBird address. Other names keep working as before.

> [!tip]
> Several Neo servers, each with its own domain like `dev.example.com` and
> `prod.example.com`? Add one zone per server, each pointing at that server's
> NetBird address.

> [!check] The server answers its app names
> The server checks this itself and says what is missing.
> ```neo-check
> systemctl is-active netbird netbird-check && journalctl -u netbird-check -b -n 3 --no-pager
> ```

## 4. Install NetBird on your devices

Install the NetBird app on your phone and laptop. In its settings, enter the
address of your network (for netbird.io, keep the default) and sign in with the
login from the dashboard. Leave it switched on.

> [!check] Your phone talks to the server directly
> Turn off Wi-Fi on your phone, keep NetBird on, and open one of your apps. In
> the NetBird app, the server shows as connected.

## 5. Optional: close apps to the internet

Each app has a **Reachable from** setting under **Access & security**. For apps
only your own devices should open, tick **Home network** and **NetBird** and
untick **Internet**. Strangers then get a "not found" page, while your devices
with NetBird still open the app.

> [!warning]
> Apps where you changed **Reachable from** before NetBird existed do not list
> NetBird yet. Tick it there, or NetBird devices get the "not found" page.

> [!tip]
> With a tunnel server, turn on
> [certificates only](option:rathole.certificateOnly) in Rathole: the tunnel
> then carries the certificate check and nothing else, so no app is reachable
> through it.

Everyone in your NetBird network can reach this server, including ports that
are closed to your home network. To narrow that, set **Access Control** rules in
the dashboard.

> [!check] The app is private
> On mobile data, open an app you closed to the internet: it works with NetBird
> on and shows the "not found" page with NetBird off.
