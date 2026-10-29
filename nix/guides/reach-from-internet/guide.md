Many internet providers do not let anyone connect to your home from outside, and
opening ports on the router is fiddly. [Rathole](service:rathole) goes the other
way round: your homeserver builds a tunnel **out** to a small rented server with
a public address (a VPS). Visitors open `https://<app>.{{domain}}`, reach the
VPS, and the VPS passes them through the tunnel to your server at home. Nothing
on your router changes.

The tunnel also carries the check for HTTPS certificates, so your apps get
proper certificates.

## What you need

- A VPS with a public IP address. The smallest one of any hosting company is
  enough, because it only passes traffic on.
- The VPS running a second Neo with the **Streamproxy** service, or another
  rathole server you set up yourself.
- Access to your domain's DNS settings at the company where you bought
  `{{domain}}`.

## 1. Pick a name and a password

Open [Rathole](service:rathole) on this server and switch it on. Fill in:

- [Homeserver name](option:rathole.name): a short name for this server, for
  example `home`. The VPS uses it to tell tunnels apart.
- [Tunnel password](option:rathole.token): press **Generate**. Both sides need
  the same value, so copy it.
- [Tunnel server address](option:rathole.remoteAddr): the IP address or name of
  your VPS.

Leave the [tunnel server port](option:rathole.port) at `2223`, the port
Streamproxy listens on.

Apply the change. The tunnel keeps trying until the VPS answers, so it is fine
to set up the VPS next.

## 2. Prepare the VPS

On the Neo that runs on the VPS, open **Streamproxy**, switch it on and add one
entry under **Tunnels**:

- **Name** of the entry: exactly the homeserver name from step 1.
- **Domain:** `{{domain}}`.
- **Tunnel password:** the same password as in step 1.
- **Include subdomains:** on, so `photos.{{domain}}` and every other app go
  through the tunnel.
- **Include the domain itself:** leave it on.

Apply the change on the VPS. Streamproxy opens ports 80, 443 and 2223 on the
VPS.

> [!warning]
> Some hosting companies have their own firewall in their web console. Allow
> incoming TCP on ports 80, 443 and 2223 there as well.

> [!tip]
> Using another rathole server instead of Streamproxy? It needs two services
> named `<homeserver name>_http` and `<homeserver name>_https`, both with the
> tunnel password. The HTTPS traffic must arrive with the PROXY protocol, as
> Streamproxy sends it, because your server's web gateway expects it on that
> tunnel.

## 3. Point your domain at the VPS

In your domain's DNS settings, create two **A** records with the VPS's IP
address:

- `{{domain}}`
- `*.{{domain}}`, which covers every app address.

DNS changes can take up to an hour to spread.

> [!check] The tunnel is up
> On your phone, turn off Wi-Fi and open one of your apps at
> `https://<app>.{{domain}}`. The login page or the app opens with a valid
> certificate.
> ```neo-check
> systemctl is-active rathole
> ```

## 4. Decide which apps the internet may see

Every app is now reachable from the internet, behind the shared login page if
the app has **Require login** on. Each app has a **Reachable from** setting under
**Access & security**:

- **Home network:** devices at home.
- **Tailscale:** your own devices anywhere, see
  [Keep your app traffic private with Tailscale](guide:private-traffic-tailscale).
- **Internet:** everyone, through the tunnel.

Untick **Internet** for apps only your household needs. Visitors from the
internet then get a "not found" page. Certificates keep renewing either way.

> [!warning]
> Your domain now points at the VPS, so devices at home also take the tunnel
> and count as **Internet**. Before you untick it, give them a direct way to the
> server: [Pi-hole](service:pihole) with its
> [home network address](option:pihole.localIP) set (see
> [Block ads for the whole home](guide:adblock-at-home)), or Tailscale.

> [!tip]
> Want the tunnel only for certificates, with no app reachable from the
> internet? Turn on
> [certificates only](option:rathole.certificateOnly). It needs Pi-hole with its
> home network address set, or Tailscale split DNS, so your devices still find
> the apps.
