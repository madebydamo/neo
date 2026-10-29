Many YouTube videos start with a sponsor message, an intro or a "please
subscribe" reminder. [iSponsorBlockTV](service:isponsorblocktv) watches along
with the YouTube app on your TV and jumps over those parts by itself. It uses
SponsorBlock, a list of segments marked by other viewers, and also skips ads as
soon as YouTube allows it.

Nothing is installed on the TV: the server connects to the YouTube app the same
way a phone does when you cast a video to it.

## What you need

- A TV or box with the YouTube app. It works with Apple TV, Roku, Chromecast,
  Google TV, Fire TV, Android TV, Samsung and LG TVs, Xbox, PlayStation,
  Nintendo Switch and more.
- The TV switched on, with the YouTube app open.
- About 10 minutes.

## 1. Turn on iSponsorBlockTV

Open [iSponsorBlockTV](service:isponsorblocktv), switch it on and apply the
change. It waits quietly until you add a TV in the next step.

## 2. Pair your TV

Open {{service.isponsorblocktv.url}} and sign in with your homeserver login.
The setup page looks like a text screen: use the keyboard, or click the buttons
on it.

1. On the TV, open the YouTube app, go to its **Settings** and choose **Link
   with TV code**. The TV shows a code.
2. On the setup page, add a new device and enter that code.
3. Add more TVs the same way, then save and close the setup.

> [!tip]
> The setup page lets you choose what to skip (sponsors, intros, reminders and
> more) and whether ads should be skipped and muted.

> [!warning]
> Only one person can use the setup page at a time. After you close it, it
> restarts and is ready again within a few seconds. Reload the page if it
> looks empty.

## 3. Watch something

After the setup closes, iSponsorBlockTV restarts with your TVs and runs on its
own from then on, also after the server restarts.

> [!check] Sponsors are skipped
> Play a YouTube video on the TV that has a sponsor segment. When the segment
> starts, the video jumps ahead by itself.
> ```neo-check
> systemctl is-active docker-isponsorblocktv isponsorblocktv-setup
> ```

## If a TV stops skipping

TVs sometimes forget the link, for example after an update of the YouTube app.
Open the setup page again and pair the TV with a new code.
