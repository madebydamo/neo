Photos are more fun when others see them too. [Immich](service:immich) shares
albums in two ways: with people who have an account on your Immich, and with
anyone through a link. A link can also let others **add** photos, so after a
wedding or birthday every guest's pictures end up in one album.

## What you need

- [Immich](service:immich) switched on, with your own Immich account. See
  [Leave Google Photos](guide:leave-google-photos) to set it up.
- About 15 minutes.

## 1. Share an album with your family

Family members who use your Immich each have their own account (see
[Leave Google Photos](guide:leave-google-photos), step 2). Share an album with
them inside Immich:

1. Open the album at {{service.immich.url}} and click **Share**.
2. Pick the people and choose whether they may only look or also add photos.

The album then shows up under **Sharing** in their Immich app, and new photos
appear for everyone.

> [!tip]
> **Partner sharing** (in your account settings) shows all your photos in your
> partner's timeline, without picking albums one by one.

## 2. Send friends a link

For people without an account, create a **shared link**: open the album, click
**Share**, then **Create link**. You can give the link:

- a **password**, which you tell people separately,
- an **expiry date**, so it stops working after a few weeks,
- **Allow download**, so friends can save the originals.

Friends open the link in any browser and see only that album. They do not pass
your homeserver login and see nothing else on your server.

Links only work from outside your home when Immich is reachable from the
internet: keep **Internet** ticked in [Reachable from](option:immich.ingress).
See [Reach your apps from the internet](guide:reach-from-internet) if your
apps do not open from outside yet.

> [!check] The link opens from outside
> Turn off Wi-Fi on your phone and open the link. The album shows without the
> homeserver login.
> ```neo-check
> systemctl is-active docker-immich-server
> ```

## 3. Collect photos from a party

After a wedding, birthday or holiday everyone has photos on their phone, and
nobody sends them. Create an empty album, for example **Anna & Ben's wedding**,
and a shared link for it with **Allow public user to upload** switched on.

Share the link in the party's group chat, by email or as a QR code on the
table. Guests open it on their phone, pick photos and videos and upload them.
They need no account and no app.

> [!warning]
> Anyone who has an upload link can add pictures to that album until the link
> expires. Give links you post in big groups a password or an expiry date, and
> switch off uploads once the party photos are in.

> [!check] Photos arrive in the album
> Open the link on your own phone and upload one picture. It shows up in the
> album at {{service.immich.url}}.
