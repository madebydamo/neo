Your photos show your family, your home and every place you have been.
[Immich](service:immich) keeps them on your own server and still feels like
Google Photos: the phone backs up new pictures by itself, and you find them
again by face, place or anything you type. Face recognition and search run on
your server, so no picture goes to a cloud service. See [Photos](privacy:photos).

## What you need

- Enough free disk space on the server for all your photos and videos.
- The **Immich** app from the App Store or Google Play on each phone.
- About 30 minutes, plus time for the first backup to finish.

## 1. Turn on Immich

Open [Immich](service:immich), switch it on and apply the change. The first
start takes a few minutes while Immich downloads and starts its parts.

Then open {{service.immich.url}}. After your homeserver login, Immich asks you
to create its first account. That account is the Immich administrator, so use
your own email address and a strong password.

> [!check] Immich is running
> The Immich page opens and shows an empty photo timeline.
> ```neo-check
> systemctl is-active docker-immich-server docker-immich-machine-learning docker-immich-database docker-immich-redis
> ```

## 2. Add your family

In Immich's administration pages, create a user for each family member. Everyone
gets their own private timeline, and you can share albums with each other later.

## 3. Back up your phone

Open the Immich app on the phone and enter the server address
`{{service.immich.url}}`. Sign in with that person's Immich email and password.

Turn on backup in the app and pick which albums to back up, usually the camera
album. The first backup can take hours for a big library: leave the phone
plugged in and on Wi-Fi.

> [!tip]
> The app does not need your homeserver login. It talks to Immich directly, so
> backup also works when you are away from home.

> [!check] Photos arrive
> Take a new photo with the phone. A little later it shows up in the timeline
> at {{service.immich.url}}.

## 4. Bring over photos that are only in Google

Photos that are still on your phone are backed up in step 3. For older photos
that only Google has, download a copy of your library with **Google Takeout**
(takeout.google.com, select only Google Photos). Unzip the download and upload
the pictures on the Immich web page.

> [!tip]
> Takeout keeps some details, such as when a photo was taken, in separate files
> next to the pictures. The community tool **immich-go** reads them and uploads
> a whole Takeout download with the right dates and albums. It needs an API
> key, which you create in your Immich account settings.

## 5. Turn off Google backup

Once everything is in Immich, switch off backup in the Google Photos app, so
new photos only go to your own server. Delete the photos at Google only when
you are sure Immich has them all.

> [!warning]
> Your photos now live on your server. Keep a backup of the server's data on
> another disk or in another place, so a broken disk does not take your
> memories with it.

## Next

Want guests to add their photos from a wedding or party? See
[Collect photos from guests after a party](guide:party-photo-drop).
