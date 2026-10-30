# Hermes skill for Dawarich.
{...}: {
  flake.modules.nixos.dawarich-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.dawarich;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.dawarich.skill.conf = lib.neo.mkServiceSkill {
      service = "dawarich";
      inherit cfg domain;
      description = "Dawarich location history, map, trips, tracking API";
      tags = ["neo" "dawarich" "location" "map"];
      title = "Neo · Dawarich";
      body = ''
        ## When to Use
        Location history, the map, trips, visits, stats, phone tracking, container health.

        ## Architecture notes
        - Containers: `dawarich` (Rails, port 3000), `dawarich-sidekiq` (jobs), `dawarich-db` (PostGIS 17), `dawarich-redis` (Redis 7.4)
        - Images `containers.dawarich` and `containers.dawarich-sidekiq` must be the same tag. Default is `freikin/dawarich:1.15.3` (pinned). Upstream says to read release notes before upgrading. Do not flip either container to `latest` on its own
        - Public URL is `https://<subdomain>.<domain>`. Neo sets `APPLICATION_PROTOCOL=https`, `DOMAIN` to that host, and `APPLICATION_HOSTS` to localhost, 127.0.0.1, ::1, that host, and any custom domains. Hostnames only, no scheme
        - `TIME_ZONE` follows `neo.core.timeZone`
        - `/api/` is on publicPaths (tinyauth bypass). Dawarich still requires its API key or a session. `GET /api/v1/health` returns `{"status":"ok"}`
        - Tracker endpoints: `POST /api/v1/owntracks/points`, `POST /api/v1/overland/batches`, `POST /api/v1/traccar/points`, each with `?api_key=`
        - The web UI, `/cable`, and share links (`/s/`, `/shared/`, `/invitations/`) stay behind tinyauth. To publish a share link, add those path regexes to `auth.publicPaths` and also `^/assets/` so the page can load CSS and JS
        - Live map updates use `/cable`. SWAG `proxy.conf` already sets Upgrade and Connection
        - Appdata: `pgdata` (Postgres 17, `/var/lib/postgresql/data`), `redisdata`, `public` (compiled assets), `storage` (uploads), `watched` (drop folder for imports)
        - The app entrypoint migrates and seeds on every start. Sidekiq starts after the app unit and waits until Postgres accepts connections

        ## Credentials
        - Neo: `services.dawarich.secretKeyBase` (SECRET_KEY_BASE, 32+ characters), `dbPassword` (internal database only)
        - First user signs up on the Dawarich login page (normal onboarding)
        - API key: Dawarich → Settings → Account. Not stored by Neo
        - Edge: tinyauth

        ## Procedures
        1. Health-check units `docker-dawarich`, `docker-dawarich-sidekiq`, `docker-dawarich-db`, `docker-dawarich-redis`
        2. Inside the app container: `wget -qO - --header=X-Forwarded-Proto:https http://127.0.0.1:3000/api/v1/health` (plain HTTP is a 301)
        3. Open the UI via tinyauth, sign in, change the seeded password
        4. Point a tracker at `https://dawarich.<domain>/api/v1/owntracks/points?api_key=<key>` (or the Overland / Traccar paths)

        ## Pitfalls
        - `secretKeyBase` left as CHANGE_ME, or shorter than 32 characters, fails the Nix assertion
        - Rotating `secretKeyBase` signs everyone out and can make encrypted settings unreadable
        - `dbPassword` is not the web UI password
        - A hostname missing from `APPLICATION_HOSTS` makes production return a host error. Custom domains on the service are included. Raw IP access is not
        - `APPLICATION_PROTOCOL=https` enables `force_ssl`. A direct `http://dawarich:3000` request is a 301 unless it sends `X-Forwarded-Proto: https`. SWAG does
        - ARM hosts: if `postgis/postgis:17-3.5-alpine` has no image for the arch, set `containers.dawarich-db` to `imresamu/postgis:17-3.5-alpine` (upstream's note)
        - Do not clear appdata without confirmation. That deletes the database, uploads, and import drop folder
        - Upgrading the image without reading the Dawarich release notes can break the database. Bump both app image fields together
        - SMTP is not configured. Password reset and family invitation mail need SMTP env vars added later
        - The stack is four containers (Rails, Sidekiq, PostGIS, Redis). First start runs migrations and country seeds before `/api/v1/health` is up

        ## Verification
        - Units active
        - `GET /api/v1/health` returns 200 with `"status":"ok"` without a tinyauth cookie
        - `GET /` returns 302 to tinyauth
        - After tinyauth and the Dawarich login, the map loads
      '';
    };
  };
}
