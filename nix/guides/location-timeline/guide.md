Google Timeline knows every place you have been. [Dawarich](service:dawarich)
keeps that history on your own server instead: a map of your trips, the places
you visited and statistics per month and year. An app on your phone sends its
location to the server, and nobody else sees it.

## What you need

- Your apps already open at `https://<app>.{{domain}}`.
- A phone with a tracker app: the Dawarich app, or
  [OwnTracks](https://owntracks.org) (iPhone and Android).
- To keep tracking while you are away from home: the server reachable from the
  internet (see
  [Make your apps reachable from the internet](guide:reach-from-internet)), or
  Tailscale on the phone (see
  [Keep your app traffic private with Tailscale](guide:private-traffic-tailscale)).

## 1. Turn on Dawarich

Open [Dawarich](service:dawarich) and switch it on. Press **Generate** for the
[secret key](option:dawarich.secretKeyBase) and for the
[database password](option:dawarich.dbPassword).

> [!warning]
> Never change these two values later. A new secret key signs everyone out and
> can make stored data unreadable, and a new database password cuts Dawarich off
> from its data.

Apply the change. The first start prepares the database and takes a few minutes.

> [!check] Dawarich is running
> ```neo-check
> systemctl is-active docker-dawarich docker-dawarich-sidekiq docker-dawarich-db docker-dawarich-redis
> docker exec swag curl -sS -m 10 -H 'X-Forwarded-Proto: https' http://dawarich:3000/api/v1/health   # {"status":"ok"}
> ```

## 2. Create your account

Open {{service.dawarich.url}}. First you pass the shared login page
([Tinyauth](service:tinyauth)), then Dawarich shows its own login. Click
**Sign up** and create your account with your email address and a password.
Family members who want their own timeline sign up the same way.

## 3. Copy your API key

Your phone does not go through the login page. It proves who it is with a
Dawarich **API key** instead. Find the key in Dawarich's account settings and
copy it.

> [!tip]
> Treat the API key like a password: anyone who has it can add points to your
> map.

## 4. Connect your phone

**With the Dawarich app:** enter your server address `{{service.dawarich.url}}`
and the API key.

**With OwnTracks:** switch the connection mode to **HTTP** and enter this
address, with your key at the end:

```
{{service.dawarich.url}}/api/v1/owntracks/points?api_key=YOUR_API_KEY
```

Allow the app to use your location in the background, otherwise it only sends
points while it is open.

> [!check] Points arrive
> Walk around the block with the phone, then open {{service.dawarich.url}}. Your
> route shows up on the map.

> [!tip]
> Dawarich can also import a Google Takeout export of your old location
> history, so the map starts with the years you already have.
