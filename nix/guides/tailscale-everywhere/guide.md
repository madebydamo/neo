Your apps live at addresses like `https://photos.{{domain}}`. With this setup
the **same address** works everywhere:

- **At home**, your devices go straight to the server, fast and without a detour
  through the internet.
- **On the road**, your phone and laptop reach the server through
  [Tailscale](service:tailscale), a private network only your own devices can
  join.

Nobody else on the internet can open your apps.

## What you need

- A free Tailscale account (sign in with Google, Microsoft, Apple or GitHub).
- The Tailscale app on the phones and laptops you want to use away from home.
- Ad blocking set up as in [Block ads for the whole home](guide:adblock-at-home):
  Pi-hole is what makes the address work at home.

## 1. Connect the server to Tailscale

1. In the [Tailscale admin console](https://login.tailscale.com/admin/settings/keys),
   create an **auth key**.
2. Open [Tailscale](service:tailscale), switch it on and paste the key into
   [auth key](option:tailscale.authKey).
3. Turn on [split DNS](option:tailscale.splitDns). It lets your devices on
   Tailscale find your apps by name.
4. Apply the change.

The server now shows up in the admin console under **Machines**. Note its
Tailscale address, which starts with `100.`.

## 2. Give Pi-hole the home address

Open [Pi-hole](service:pihole) and set the
[home network address](option:pihole.localIP) to your server's address on the
home network, for example `192.168.1.10` (your router lists it). Pi-hole then
answers with that address at home, and Tailscale answers with the `100.`
address on the road.

> [!warning]
> Split DNS does not start until this address is set. That is on purpose:
> otherwise Pi-hole and Tailscale would answer on the same address.

## 3. Tell Tailscale where your domain lives

In the admin console open **DNS**, then **Add nameserver → Custom**:

- **Nameserver:** the server's Tailscale address (`100.…`) from step 1.
- **Restrict to domain:** `{{domain}}`.

Save. Every device in your Tailscale network now asks your server for anything
ending in `{{domain}}`.

## 4. Keep the apps private

Each app has a **Reachable from** setting under **Access & security**. For apps
that should stay private, tick **Home network** and **Tailscale**, and untick
**Internet**.

> [!tip]
> HTTPS certificates still need one way in from the internet. If you don't want
> to open port 80 on your router, use a tunnel server with
> [Rathole](service:rathole) and turn on
> [certificates only](option:rathole.certificateOnly): it lets the certificates
> through and nothing else.

## 5. Install Tailscale on your devices

Install the Tailscale app on your phone and laptop and sign in with the same
account. Leave it switched on.

> [!check] The same address works away from home
> Turn off Wi-Fi on your phone so it uses mobile data, keep Tailscale on, and
> open `https://pihole.{{domain}}`. The Pi-hole page loads.
> ```neo-check
> systemctl is-active tailscaled tailscale-split-dns dnsmasq
> ```
