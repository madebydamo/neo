Every device at home asks a "phone book" (DNS) where to find a website before it
loads it. [Pi-hole](service:pihole) becomes that phone book for your home and
simply does not answer for ads and trackers. Phones, smart TVs, game consoles and
apps get no ads, and you install nothing on them.

## What you need

- Access to your router's settings page (usually printed on the router).
- About 15 minutes.

## 1. Turn on Pi-hole

Open [Pi-hole](service:pihole), switch it on and set an
[admin password](option:pihole.webPassword). Leave the
[home network address](option:pihole.localIP) empty unless you also use
Tailscale split DNS (see
[Keep your app traffic private with Tailscale](guide:private-traffic-tailscale)).

Apply the change. After a minute the dashboard opens at
{{service.pihole.url}}.

> [!tip]
> Pi-hole comes with a good blocklist. You can add more lists in its dashboard
> under **Lists** later.

## 2. Find your server's address

Your router shows a list of connected devices. Find your homeserver and note its
address, something like `192.168.1.10`.

> [!warning]
> Ask the router to always give the server the same address. The setting is
> usually called **fixed IP**, **static lease** or **address reservation**.
> Otherwise ad blocking stops when the address changes.

## 3. Tell your router to use Pi-hole

In the router's network (DHCP or LAN) settings, set the **DNS server** to your
server's address from step 2. Leave a second DNS server empty: if it is set,
devices sometimes skip Pi-hole.

Devices pick up the new setting the next time they connect. Turn Wi-Fi off and on
on your phone to make it happen right away.

## 4. Check that it works

> [!check] Ads are gone
> Open a news site on your phone. Ad spaces stay empty, and the Pi-hole
> dashboard shows your phone under **Clients** with blocked requests.
> ```neo-check
> docker exec pihole dig +short doubleclick.net @127.0.0.1   # 0.0.0.0 = blocked
> ```

## If a site breaks

Some shops or apps stop working when a tracker is blocked. Open the Pi-hole
dashboard, find the blocked request under **Query log** and click **Allow**.
