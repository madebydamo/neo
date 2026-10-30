A VPN hides your home address from the websites you visit. Usually that means a
VPN app on every device, and some devices (a work laptop, a school tablet) do
not allow one. Here the [VPN](service:vpn) runs on your server once, and you
choose which apps go online through it: a [Firefox](service:firefox) you open
in any browser tab, your search engine, the app that checks websites for
changes, and more.

Websites see your VPN provider's address. Apps you do not route, and your
other devices, keep using your normal connection.

## What you need

- A VPN subscription that offers **WireGuard** configuration files with a
  preshared key. We recommend [AirVPN](https://airvpn.org): it works with Neo
  out of the box, is the default here, does not log what you do and lets you
  forward ports.
- About 20 minutes.

## 1. Get the WireGuard details from your VPN provider

On AirVPN, sign in and open **Client Area → Config Generator**. Choose
**Linux**, tick **WireGuard**, pick a country and generate the configuration.
Other providers call this **manual setup** or **WireGuard configuration**. You
need these lines from the file:

- `PrivateKey`
- `PresharedKey`
- `Address`: the IPv4 part, for example `10.128.12.34/32`

> [!warning]
> The private key is like a password for your VPN account. Paste it only into
> Neo and do not share the file.

## 2. Turn on the VPN

Open [VPN](service:vpn), switch it on and fill in:

- [VPN provider](option:vpn.vpnServiceProvider): `airvpn` is already set. For
  another provider, its short name in lowercase, e.g. `protonvpn`.
- [Private key](option:vpn.wireguardPrivateKey),
  [Preshared key](option:vpn.wireguardPresharedKey) and
  [Address](option:vpn.wireguardAddresses): the lines from step 1.
- [Server countries](option:vpn.serverCountries): where the VPN servers should
  be, e.g. `Switzerland` or `Netherlands,Germany`. The default is
  `Netherlands`.

Neo refuses to apply the VPN until the private key, preshared key and address
are all filled in. Apply the change.

> [!check] The VPN is connected
> The VPN app shows as running, and its log names the public address it uses.
> ```neo-check
> systemctl is-active docker-vpn
> docker logs vpn 2>&1 | grep -i 'public ip' | tail -1
> ```

## 3. A browser that goes online through the VPN

Open [Firefox](service:firefox) and switch it on. Under **Access & security**,
turn on [Route through VPN](option:firefox.vpn.enabled). Keep
[Require login](option:firefox.auth.enabled) on: whoever opens this browser
controls a program on your server. Apply the change.

Open {{service.firefox.url}} on any device and sign in with your homeserver
login. A complete Firefox appears inside the tab, and everything you open in it
goes online through the VPN.

> [!check] Websites see the VPN
> In the server's Firefox, search for "what is my IP". The address and country
> shown belong to your VPN provider, not your home connection.
> ```neo-check
> systemctl is-active docker-vpn docker-firefox
> ```

> [!tip]
> The server's Firefox keeps its history, cookies, logins and downloads, and it
> keeps running after you close the tab. After a private browsing session, wipe
> all traces with **Clear appdata** in the danger zone at the bottom of
> [Firefox](service:firefox)'s page. It starts again like new.

## 4. Route more apps

Every app with a **Route through VPN** switch can use the same connection. Turn
it on for the ones whose internet traffic you want to hide:

- [Webtop](service:webtop): a whole Linux desktop in a browser tab, with every
  program on it going through the VPN.
- [SearXNG](service:searxng): the search engines see your VPN address instead
  of your home.
- [Karakeep](service:karakeep): fetches the pages you save through the VPN.
- [Changedetection](service:changedetection): checks websites without
  revealing who is watching.

You open these apps as before; only their own trips to the internet take the
VPN. Several apps can use the VPN at the same time.

## If pages stop loading

When the VPN connection drops, routed apps have no internet at all, so nothing
leaks over your home connection. Check that the [VPN](service:vpn) app is
running and the WireGuard details are still valid; providers sometimes remove
old keys.
