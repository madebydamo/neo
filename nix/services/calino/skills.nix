# Hermes skill for calino.
{...}: {
  flake.modules.nixos.calino-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.calino;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.calino.skill.conf = lib.neo.mkServiceSkill {
      service = "calino";
      inherit cfg domain;
      description = "Calino browser CalDAV calendar client";
      tags = ["neo" "calino" "caldav" "calendar"];
      title = "Neo · Calino";
      body = ''
        ## When to Use
        Web calendar UI against RustiCal (or another CalDAV server), CORS, tinyauth on the SPA, ICS/webcal subscriptions.

        ## Architecture notes
        - Image: ghcr.io/ivan-malinovski/calino (port 8080), static SPA behind Caddy, no appdata
        - Edge tinyauth on the UI; no publicPaths (no dedicated health endpoint)
        - Browser talks to CalDAV directly. When RustiCal is enabled, SWAG adds CORS on DAV paths for `https://calino.<domain>` only
        - ICS/webcal subscribe is a browser GET. SWAG location `/webcal-proxy/` fetches the feed so publishers that omit CORS still work
        - Contacts use CardDAV. RustiCal keeps `/carddav` separate from `/caldav`; Neo advertises `addressbook-home-set` on the CalDAV principal so Calino can find address books without a second account URL
        - Calino Caddy sends `X-Frame-Options: SAMEORIGIN`; SWAG hides it when neo iframeCookieSupport is on so the navigator can embed the UI
        - Health: GET `/` inside the container / from SWAG → HTML 200

        ## Credentials
        - Edge: tinyauth for the Calino UI and `/webcal-proxy/`
        - CalDAV: RustiCal principal + app token (HTTP Basic), entered in Calino settings and stored in the browser
        - Calino has no server-side accounts

        ## Procedures
        1. `systemctl status docker-calino`
        2. `curl http://calino:8080/` (from the internal network / SWAG container)
        3. Open the public URL, pass tinyauth
        4. In Calino settings, add CalDAV URL `https://rustical.<domain>/caldav` plus a RustiCal app token
        5. To subscribe to a public `.ics` / webcal feed: Settings → Sync → Subscribe to calendar, paste the feed URL, expand **Proxy URL** and enter `https://calino.<domain>/webcal-proxy`
        6. If calendars do not load, run Settings → Sync → Diagnose (usually CORS or token)

        ## Pitfalls
        - Different subdomain than RustiCal: CORS is required. Neo only adds it on RustiCal when Calino is enabled
        - Native clients still use RustiCal app tokens; Calino is not a CalDAV server
        - Clearing browser data drops Calino's local accounts (server calendars are unaffected)
        - Connecting to a CalDAV server other than this host's RustiCal needs CORS on that server (do not point CalDAV at `/webcal-proxy/`)
        - Subscribing without the Proxy URL fails when the publisher omits `Access-Control-Allow-Origin` (typical)
        - Diagnose REPORT 405 on `https://rustical.<domain>/caldav` is expected: that URL is the DAV root, not a calendar collection. Calino still queries calendars under `/caldav/principal/<id>/`

        ## Verification
        - Internal GET `/` returns 200
        - Public `/` redirects to tinyauth
        - OPTIONS `https://rustical.<domain>/caldav/` with `Origin: https://calino.<domain>` returns 204 and CORS allow headers
        - After tinyauth + app token, calendars load in the UI
        - After tinyauth, GET `https://calino.<domain>/webcal-proxy/https%3A%2F%2Fi.cal.to/ical/564/fcsg/spielplan/71a6a4c6.d7cbfa80-958590ba.ics` returns `text/calendar`
      '';
    };
  };
}
