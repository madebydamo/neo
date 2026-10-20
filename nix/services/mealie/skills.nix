# Hermes skill for Mealie.
{...}: {
  flake.modules.nixos.mealie-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.mealie;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.mealie.skill.conf = lib.neo.mkServiceSkill {
      service = "mealie";
      inherit cfg domain;
      description = "Mealie recipes, meal plans, shopping lists, API";
      tags = ["neo" "mealie" "recipes" "food"];
      title = "Neo · Mealie";
      body = ''
        ## When to Use
        Recipes, cookbooks, meal plans, shopping lists, URL import, API tokens, container health.

        ## Architecture notes
        - One container: `mealie` (ghcr.io/mealie-recipes/mealie, port ${toString cfg.port})
        - Image `containers.mealie` is pinned to v3.29.0. Upstream says to read the release notes before upgrading. Do not flip it to `latest` on its own
        - SQLite. Appdata is mounted at `/app/data` (database and uploads). Neo sets `PUID`/`PGID` from `neo.core.uid`/`gid` and `TZ` from `neo.core.timeZone`
        - `BASE_URL` is `https://<subdomain>.<domain>` (default subdomain `mealie`). Used for links and OIDC callbacks
        - `/api/` is on publicPaths (tinyauth bypass). Mealie still requires its session or an API token. `GET /api/app/about` is unauthenticated and is the image healthcheck
        - The web UI stays behind tinyauth, then Mealie's own login
        - `allowSignup` maps to `ALLOW_SIGNUP` and is off by default. Invites still work from the admin UI
        - The container is capped at 1000m of RAM, matching upstream's idle-memory note

        ## Credentials
        - Neo stores no Mealie password. `services.mealie.allowSignup` is not a secret
        - First boot seeds `changeme@example.com` / `MyPassword` when the database is empty. Change that password. Later boots do not reset it
        - API tokens: Mealie → user profile → API tokens. Not stored by Neo
        - Edge: tinyauth

        ## Procedures
        1. Health-check unit `docker-mealie`
        2. `GET https://mealie.<domain>/api/app/about` (publicPaths) returns JSON with `version`
        3. Open the UI via tinyauth, sign in with the seeded account, change the password
        4. For a phone app, create an API token and point the app at the public URL. `/api/` does not require a tinyauth cookie

        ## Pitfalls
        - The seeded password is public until you change it. Do not expose the host with tinyauth disabled before that
        - `allowSignup` does not create the first user. An empty database already has the seeded account
        - `API_PORT` is the container listen port. The option is internal and defaults to 9000. Upstream says not to change it in Docker
        - A wrong `BASE_URL` breaks notification links and OIDC callbacks
        - Do not clear appdata without confirmation. That deletes the cookbook, meal plans, and uploads
        - Upgrading the image without reading the Mealie release notes can require a manual migration. Bump `containers.mealie` on purpose
        - The entrypoint `chown`s `/app`, including the data mount, to `PUID:PGID` on every start

        ## Verification
        - `docker-mealie` is active
        - `GET /api/app/about` returns 200 without a tinyauth cookie
        - `GET /` returns 302 to tinyauth
        - After tinyauth and the Mealie login, the recipe list loads
      '';
    };
  };
}
