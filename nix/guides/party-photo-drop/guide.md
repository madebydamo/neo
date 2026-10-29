After a wedding, birthday or holiday, everyone has photos on their phone, and
nobody sends them. With [Immich drop](service:immich-drop) you send guests one
link. They open it, pick their photos and videos, and the pictures land in an
album in your [Immich](service:immich). Guests need no account and never see
anything else on your server.

## What you need

- [Immich](service:immich) switched on, with your own Immich account. See
  [Leave Google Photos](guide:leave-google-photos) to set it up.
- About 15 minutes.

## 1. Turn on Immich drop

Open [Immich drop](service:immich-drop), switch it on and apply the change. It
opens at {{service.immich-drop.url}}.

Guests must be able to open this page from anywhere, so keep **Internet** ticked
in [Reachable from](option:immich-drop.ingress). The page has no homeserver
login on purpose: only people with an invite link can upload.

> [!check] Immich drop is running
> {{service.immich-drop.url}} opens in the browser.
> ```neo-check
> systemctl is-active docker-immich-drop docker-immich-server
> ```

## 2. Create an invite link

Sign in at {{service.immich-drop.url}} with your **Immich** email and password.
Create an invite link and choose the album the photos should go to, for example
**Anna & Ben's wedding**. If the album does not exist yet, it is created.

You can also give the link:

- a **password**, which you tell guests separately,
- an **expiry date**, so the link stops working after a few weeks,
- **one-time use**, for a link meant for a single person.

## 3. Send the link to your guests

Share the link in the party's group chat, by email or as a QR code on the
table. Guests open it on their phone, choose photos and videos and upload.
Large videos are sent in pieces and retried if the connection drops, and the
date each photo was taken is kept.

> [!tip]
> If two guests send the same picture, Immich drop notices the duplicate.

> [!check] Photos arrive in the album
> Open the invite link on your own phone and upload one picture. It shows up in
> the album in Immich at {{service.immich.url}}.

## 4. Share the album

When the photos are in, share the finished album with the family in Immich.

> [!warning]
> Anyone who has the link can upload to that album until it expires. Use a
> password or an expiry date for links you post in big groups.
