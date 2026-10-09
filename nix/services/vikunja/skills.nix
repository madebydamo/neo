# Hermes skill for Vikunja.
{...}: {
  flake.modules.nixos.vikunja-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.vikunja;
    appdata = cfg.appdata;
    domain = config.neo.services.swag.domain or null;
    publicUrl =
      if domain != null && domain != "" && (cfg.subdomain or null) != null
      then "https://${cfg.subdomain}.${domain}"
      else "https://<subdomain>.<domain>";
  in {
    config.neo.services.vikunja.skill.conf = lib.neo.mkServiceSkill {
      service = "vikunja";
      inherit cfg domain;
      description = "Vikunja task manager (self-hosted Todoist)";
      tags = ["neo" "vikunja" "tasks" "todo"];
      title = "Neo · Vikunja";
      body = ''
        ## When to Use
        Personal or shared tasks, projects, reminders, Todoist import, CalDAV task sync.

        ## Architecture notes
        - Package: pkgs.vikunja via services.vikunja (not a container)
        - Unit: vikunja.service (runs as the neo user homeserver), SQLite at ${appdata}/vikunja.db
        - Listens on 127.0.0.1:${toString cfg.port}; SWAG reaches it via host.docker.internal + DNAT
        - Public URL: ${publicUrl}/ (subdomain option; default tasks)
        - Web UI is behind tinyauth (GET / is 302). Do not turn edge auth off
        - publicPaths: /api (apps, JWT, API tokens, /api/v1/info), /dav (CalDAV), /.well-known/caldav, /feeds (HTTP Basic), /health
        - CalDAV base is ${publicUrl}/dav/ — not the site root. Clients use the Vikunja username and password, a CalDAV token, or an API token

        ## Procedures
        1. systemctl status vikunja
        2. Open ${publicUrl}/ (tinyauth first) and create the first user (that user is admin)
        3. Settings → Import to pull Todoist / Microsoft To Do / Trello / CSV
        4. Install the Vikunja app (Android, iOS, desktop) and point it at ${publicUrl}
        5. For CalDAV, use ${publicUrl}/dav/principals/<username>/ in DAVx5 or Apple Reminders

        ## Pitfalls
        - The NixOS module sets publicurl from frontendScheme + frontendHostname, with a trailing slash. Do not set settings.service.publicurl again
        - Todoist OAuth import needs the site publicly reachable. The callback page is the SPA (/migrate/todoist, behind tinyauth); the migration API is under /api
        - Do not remove /api, /dav, /.well-known/caldav, /feeds, or /health from auth.publicPaths
        - Database and uploads live in Neo appdata (${appdata}, files under files/), so Clear-appdata and volume snapshots cover them. The module's /var/lib/vikunja is not used

        ## Verification
        - systemctl is-active vikunja
        - curl -fsS http://127.0.0.1:${toString cfg.port}/api/v1/info
        - ${publicUrl}/ returns 302 to tinyauth
        - ${publicUrl}/api/v1/info is on publicPaths (200, no tinyauth cookie)
      '';
    };
  };
}
