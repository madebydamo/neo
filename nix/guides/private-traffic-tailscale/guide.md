Away from home, your phone normally reaches an app like `https://photos.{{domain}}`
over the public internet, through your router or a tunnel server. With
[Tailscale](service:tailscale) on the server and on your devices, they talk to
the server **directly**, over a private, encrypted connection that only your own
devices can join. The addresses and HTTPS certificates stay the same, so nothing
changes for the apps.

Once that works, you can close apps to the internet completely and still use
them from anywhere.

## What you need

- **Your apps already open at `https://<app>.{{domain}}`.** HTTPS certificates
  still come from the web gateway ([SWAG](service:swag)), and to get and renew
  them the server must be reachable from the internet on port 80: either a
  public IP address with port 80 forwarded on your router, or a tunnel server
  with [Rathole](service:rathole).
- A free Tailscale account (sign in with Google, Microsoft, Apple or GitHub).
- The Tailscale app on the phones and laptops you want to use.

## 1. Connect the server to Tailscale

1. In the [Tailscale admin console](https://login.tailscale.com/admin/settings/keys),
   create an **auth key**.
2. Open [Tailscale](service:tailscale), switch it on and paste the key into
   [auth key](option:tailscale.authKey).
3. Turn on [split DNS](option:tailscale.splitDns). The server then answers
   "where is `photos.{{domain}}`?" with its own Tailscale address.
4. Apply the change.

The server now shows up in the admin console under **Machines**. Note its
Tailscale address, which starts with `100.`.

> [!warning]
> If [Pi-hole](service:pihole) is switched on, set its
> [home network address](option:pihole.localIP) first. Pi-hole and split DNS
> both answer name lookups, and Neo refuses to apply the change until each has
> its own address.

## 2. Send your domain to the server

In the admin console open **DNS**, then **Add nameserver → Custom**:

- **Nameserver:** the server's Tailscale address (`100.…`) from step 1.
- **Restrict to domain:** `{{domain}}`.

Save. Every device in your Tailscale network now asks your server for anything
ending in `{{domain}}`, and connects to it directly.

## 3. Install Tailscale on your devices

Install the Tailscale app on your phone and laptop, sign in with the same account
and leave it switched on.

> [!check] Your phone talks to the server directly
> Turn off Wi-Fi on your phone, keep Tailscale on, and open one of your apps.
> In the Tailscale app, the server shows as connected and traffic is going to it.
> ```neo-check
> systemctl is-active tailscaled tailscale-split-dns dnsmasq
> ```

## 4. Optional: close apps to the internet

Each app has a **Reachable from** setting under **Access & security**. For apps
only your own devices should open, tick **Home network** and **Tailscale** and
untick **Internet**. Strangers then get a "not found" page, while your devices
with Tailscale still open the app.

Certificates keep renewing, because the certificate check on port 80 still
reaches the server.

> [!tip]
> With a tunnel server, turn on
> [certificates only](option:rathole.certificateOnly) in Rathole: the tunnel
> then carries the certificate check and nothing else, so no app is reachable
> through it.

> [!check] The app is private
> On mobile data, open an app you closed to the internet: it works with Tailscale
> on and shows the "not found" page with Tailscale off.
