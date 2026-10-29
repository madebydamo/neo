Everything you type into a search engine tells it what worries you, what you
want to buy and where you plan to go. [SearXNG](service:searxng) runs on your
own server and asks many search engines at once for you. The results come back
combined and without ads, and the search engines only see your server, not a
profile of you.

Set it as the default search engine in your browser, and searching from the
address bar works just like before.

## What you need

- A computer or phone with Firefox, Chrome, Edge or another browser.
- About 10 minutes.

## 1. Turn on SearXNG

Open [SearXNG](service:searxng), switch it on and apply the change. After a
minute the search page opens at {{service.searxng.url}}.

> [!check] Search works
> Open {{service.searxng.url}} and search for something. Results from several
> search engines show up.
> ```neo-check
> systemctl is-active docker-searxng docker-searxng-redis
> ```

## 2. Decide who may use it

Out of the box, SearXNG opens **without** your homeserver login, so the address
bar search works right away on every device. Anyone who knows the address can
search with it too.

To keep it to your household, pick one of these under **Access & security**:

- Turn on [Require login](option:searxng.auth.enabled). Each browser then asks
  for your homeserver login once, and searches work normally afterwards.
- Or limit [Reachable from](option:searxng.ingress) to **Home network** and
  **Tailscale**, so strangers on the internet cannot open it (see
  [Keep your app traffic private with Tailscale](guide:private-traffic-tailscale)).

> [!tip]
> If you use the [VPN](service:vpn) app, turn on
> [Route through VPN](option:searxng.vpn.enabled). The search engines then see
> your VPN address instead of your home connection.

## 3. Make it your browser's default search

Every browser needs the same piece of information, the **search address**:

```
{{service.searxng.url}}/search?q=%s
```

`%s` is the spot where the browser puts the words you type.

- **Firefox:** open {{service.searxng.url}} once. Firefox then offers SearXNG as
  a search engine you can add (look in the address bar menu, or under
  **Search** in the settings). Add it and choose it as the default search
  engine.
- **Chrome and Edge:** in the settings, open the search engine section, add a
  new search engine with the name **SearXNG** and the search address above,
  then make it the default.
- **Safari:** Safari only lets you choose from its own list of search engines.
  Bookmark {{service.searxng.url}} instead, or use Firefox on that device.
- **Phones:** Firefox and Chrome on Android and iPhone have the same search
  engine settings as on the computer.

> [!check] The address bar searches privately
> Type a few words into the address bar and press Enter. The results page
> opens at `{{service.searxng.url}}`, not at Google or Bing.

## If results are missing

Sometimes a single search engine refuses to answer for a while. SearXNG just
shows results from the others. If no results show up at all, check that the
server is online and, when you use the VPN, that the [VPN](service:vpn) app is
connected.
