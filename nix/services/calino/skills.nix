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
        Web calendar UI against RustiCal (or another CalDAV server), CORS, tinyauth on the SPA.

        ## Architecture notes
        - Image: ghcr.io/ivan-malinovski/calino (port 8080), static SPA behind Caddy, no appdata
        - Edge tinyauth on the UI; no publicPaths (no dedicated health endpoint)
        - Browser talks to CalDAV directly. When RustiCal is enabled, SWAG adds CORS on DAV paths for `https://calino.<domain>` only
        - Contacts use CardDAV. RustiCal keeps `/carddav` separate from `/caldav`; Neo advertises `addressbook-home-set` on the CalDAV principal so Calino can find address books without a second account URL
        - Calino Caddy sends `X-Frame-Options: SAMEORIGIN`; SWAG hides it when neo iframeCookieSupport is on so the navigator can embed the UI
        - Health: GET `/` inside the container / from SWAG → HTML 200

        ## Credentials
        - Edge: tinyauth for the Calino UI
        - CalDAV: RustiCal principal + app token (HTTP Basic), entered in Calino settings and stored in the browser
        - Calino has no server-side accounts

        ## Procedures
        1. `systemctl status docker-calino`
        2. `curl http://calino:8080/` (from the internal network / SWAG container)
        3. Open the public URL, pass tinyauth
        4. In Calino settings, add CalDAV URL `https://rustical.<domain>/caldav` plus a RustiCal app token
        5. If calendars do not load, run Settings → Sync → Diagnose (usually CORS or token)

        ## Pitfalls
        - Different subdomain than RustiCal: CORS is required. Neo only adds it on RustiCal when Calino is enabled
        - Native clients still use RustiCal app tokens; Calino is not a CalDAV server
        - Clearing browser data drops Calino's local accounts (server calendars are unaffected)
        - Connecting to a CalDAV server other than this host's RustiCal needs CORS on that server (no Calino proxy container)
        - Diagnose REPORT 405 on `https://rustical.<domain>/caldav` is expected: that URL is the DAV root, not a calendar collection. Calino still queries calendars under `/caldav/principal/<id>/`

        ## Verification
        - Internal GET `/` returns 200
        - Public `/` redirects to tinyauth
        - OPTIONS `https://rustical.<domain>/caldav/` with `Origin: https://calino.<domain>` returns 204 and CORS allow headers
        - After tinyauth + app token, calendars load in the UI
      '';
    };
  };
}
