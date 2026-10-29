A VPN hides your home address from the websites you visit, but it usually means
an app on every device, and some devices (a work laptop, a school tablet) do
not allow one. Here the VPN runs on your server instead. [Firefox](service:firefox)
also runs on the server, goes online only through the [VPN](service:vpn), and
you open it in a normal browser tab on any device.

Websites see your VPN provider's address. Your other apps and devices keep
using your normal connection.

## What you need

- A VPN subscription that offers **WireGuard** configuration files.
- About 20 minutes.

## 1. Get the WireGuard details from your VPN provider

On your VPN provider's website, create a WireGuard configuration (often called
**config generator** or **manual setup**) and download or open the file. You
need these lines from it:

- `PrivateKey`
- `PresharedKey`
- `Address`, for example `10.64.1.2/32`

> [!warning]
> The private key is like a password for your VPN account. Paste it only into
> Neo and do not share the file.

## 2. Turn on the VPN

Open [VPN](service:vpn), switch it on and fill in:

- [VPN provider](option:vpn.vpnServiceProvider): your provider's short name in
  lowercase, e.g. `mullvad`, `protonvpn` or `airvpn`. The default is `airvpn`.
- [Private key](option:vpn.wireguardPrivateKey),
  [Preshared key](option:vpn.wireguardPresharedKey) and
  [Address](option:vpn.wireguardAddresses): the lines from step 1.
- [Server countries](option:vpn.serverCountries): where the VPN servers should
  be, e.g. `Switzerland` or `Netherlands,Germany`. The default is
  `Netherlands`.

Neo refuses to apply the VPN until the private key, preshared key and address
are all filled in. Apply the change.

## 3. Turn on Firefox and route it through the VPN

Open [Firefox](service:firefox) and switch it on. Under **Access & security**,
turn on [Route through VPN](option:firefox.vpn.enabled). Keep
[Require login](option:firefox.auth.enabled) on: whoever opens this browser
controls a program on your server. Apply the change.

> [!check] The VPN is connected
> The VPN app shows as running, and its log names the public address it uses.
> ```neo-check
> systemctl is-active docker-vpn docker-firefox
> docker logs vpn 2>&1 | grep -i 'public ip' | tail -1
> ```

## 4. Browse through the VPN

Open {{service.firefox.url}} on any device and sign in with your homeserver
login. A complete Firefox appears inside the tab. Everything you open in it
goes online through the VPN.

> [!check] Websites see the VPN
> In the server's Firefox, search for "what is my IP". The address and country
> shown belong to your VPN provider, not your home connection.

> [!tip]
> The server's Firefox keeps its bookmarks, logins and downloads between
> sessions, and it keeps running after you close the tab. Pick up where you
> left off from another device.

## Optional: a whole desktop through the VPN

[Webtop](service:webtop) is a complete Linux desktop in a browser tab. Turn on
its [Route through VPN](option:webtop.vpn.enabled) too, and every program on
that desktop goes online through the VPN. Firefox and Webtop can both use the
VPN at the same time.

## If pages stop loading

When the VPN connection drops, the browser has no internet at all, so nothing
leaks over your home connection. Check that the [VPN](service:vpn) app is
running and the WireGuard details are still valid; providers sometimes remove
old keys.
