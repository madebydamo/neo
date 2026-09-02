# Hermes skill for rustical.
{...}: {
  flake.modules.nixos.rustical-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.rustical;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.rustical.skill.conf = lib.neo.mkServiceSkill {
      service = "rustical";
      inherit cfg domain;
      description = "RustiCal CalDAV/CardDAV calendars and contacts";
      tags = ["neo" "rustical" "caldav" "carddav" "calendar"];
      title = "Neo · RustiCal";
      body = ''
        ## When to Use
        Calendar/contact sync (DAVx5, Apple, Thunderbird, Evolution), principals, app tokens, SQLite backup.

        ## Architecture notes
        - Image: ghcr.io/lennart-k/rustical (port 4000), SQLite at `/var/lib/rustical/db.sqlite3`
        - CalDAV `/caldav` (Apple: `/caldav-compat`), CardDAV `/carddav`
        - Edge tinyauth on the web UI (`/frontend`); DAV, well-known, Nextcloud login flow, and `/ping` are on publicPaths
        - PROPFIND/OPTIONS/REPORT on `/` skip tinyauth in nginx (308 → `/.well-known/caldav`). GET `/` stays behind tinyauth — `/` is not a publicPath
        - When Calino is enabled, SWAG adds CORS on DAV paths for `https://calino.<domain>` and answers OPTIONS itself only when Origin is Calino (DAVx5 OPTIONS must reach RustiCal for the DAV header)
        - CalDAV and CardDAV are separate URL trees. SWAG rewrites CalDAV principal PROPFIND so `addressbook-home-set` points at `/carddav/principal/<id>/` (Calino/tsdav expects a unified DAV principal)
        - Health: GET `/ping` → `Pong!`
        - With `ssoPassword` set, Neo provisions a principal per tinyauth user and SWAG completes `/frontend/login` as `Remote-User` — no second login form

        ## Credentials
        - Edge: tinyauth for the frontend (the only interactive login when SSO is on)
        - `services.rustical.ssoPassword`: internal secret for that auto-login, not a CalDAV password
        - CalDAV/CardDAV clients: user id + generated app token (HTTP Basic); tokens from the frontend

        ## Procedures
        1. `systemctl status docker-rustical rustical-provision`
        2. `curl http://rustical:4000/ping` (from the internal network / SWAG container)
        3. Open the public URL, pass tinyauth — the RustiCal UI should load as that username
        4. Generate an app token in the frontend
        5. Point clients at `https://rustical.<domain>/` (root PROPFIND discovers CalDAV) or `https://rustical.<domain>/caldav` (Apple: `/caldav-compat`)
        6. For public iCal feeds, enable `ical-sync` and paste that app token there (not ssoPassword)

        ## Pitfalls
        - User ids cannot contain `:` or `$`
        - Browser CalDAV (Calino) needs CORS on DAV; native clients do not
        - REPORT on `/caldav` (the root) is 405; calendar collections under `/caldav/principal/<id>/` accept REPORT
        - Apple Calendar needs `/caldav-compat` and often a downloaded configuration profile
        - Nextcloud login flow (DAVx5) hits `/index.php/login/v2` then `/frontend/login` — the login page is behind tinyauth
        - Clearing appdata destroys calendars, contacts, and principals
        - Run behind HTTPS (RustiCal session cookies are Secure; Apple Calendar expects TLS)

        ## Verification
        - `/ping` returns `Pong!` (200) without tinyauth
        - GET `/` redirects to tinyauth
        - PROPFIND `/` returns 308 to `/.well-known/caldav` (not tinyauth HTML)
        - A client with an app token can PROPFIND `/caldav`
        - With Calino enabled, OPTIONS `/caldav/` with Calino Origin returns 204 + CORS headers
        - OPTIONS `/caldav/` without Origin reaches RustiCal (DAV header present)
      '';
    };
  };
}
