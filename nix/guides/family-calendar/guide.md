Calendars say a lot about a family: who meets whom, when you are away, where
the children go to school. With [RustiCal](service:rustical) they stay on your
own server, and every phone still syncs them like it would with Google or iCloud.

## 1. Turn on RustiCal

Open [RustiCal](service:rustical) and switch it on. Generate the
[single sign-on secret](option:rustical.ssoPassword), so the RustiCal page opens
right after your homeserver login without a second password. Apply the change.

Open `{{service.rustical.url}}` and create a calendar, for example **Family**.

## 2. Create a password for each phone

Phones do not use your normal login. In RustiCal, open **App tokens** and create
one token per phone ("Anna's iPhone") and copy it. Your user name is the one you
use to sign in to the homeserver.

> [!tip]
> One token per device means you can switch off a lost phone without changing
> the others.

## 3. Add the calendar to the phones

**iPhone and iPad:** Settings → Calendar → Accounts → Add Account → Other →
**Add CalDAV Account**. Server `{{service.rustical.url}}`, your user name, and
the app token as password.

**Android:** install **DAVx⁵**, add an account with "Login with URL and user
name", enter `{{service.rustical.url}}`, your user name and the app token. Then
pick which calendars to sync.

**Mac and Thunderbird:** add a CalDAV account with the same address, user name
and a token.

> [!check] Events show up everywhere
> Create an event on one phone. After a few seconds it appears on the others.
> ```neo-check
> systemctl is-active docker-rustical
> ```

## 4. Optional: add school holidays and other public calendars

Many schools, towns and sports clubs publish calendars as `.ics` links.
[ical-sync](service:ical-sync) copies them into your RustiCal calendars, so they
appear on every phone without anyone subscribing on their own.

Open [ical-sync](service:ical-sync), switch it on, enter your user name and a
new app token as [password](option:ical-sync.password), and add each feed under
[subscriptions](option:ical-sync.subscriptions) with a calendar name such as
`school-holidays`.

## 5. Optional: a calendar in the browser

[Calino](service:calino) shows the same calendars in a browser tab, with month,
week and day views. Handy on a computer at home.
