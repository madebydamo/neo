# Hermes skill for Vikunja.
{...}: {
  flake.modules.nixos.vikunja-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.vikunja;
    domain = config.neo.services.swag.domain or null;
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
        - Unit: vikunja.service, SQLite at /var/lib/vikunja/vikunja.db
        - Listens on 0.0.0.0:${toString cfg.port}; SWAG proxies host.docker.internal to that port
        - Public URL: https://tasks.<domain>/ (subdomain option, default tasks)
        - Own accounts. Edge tinyauth is off so the official apps and CalDAV basic-auth work
        - CalDAV base is the site root; clients use the Vikunja username and password or an app token

        ## Procedures
        1. systemctl status vikunja
        2. Open https://tasks.<domain>/ and create the first user (that user is admin)
        3. Settings → Import to pull Todoist / Microsoft To Do / Trello / CSV
        4. Install the Vikunja app (Android, iOS, desktop) and point it at the public URL
        5. For CalDAV, use the same URL in DAVx5 or Apple Reminders

        ## Pitfalls
        - publicurl must be https://<subdomain>.<domain>/ with the trailing slash, or CORS and the importer fail
        - Todoist OAuth import needs the site publicly reachable and a Todoist app whose redirect is https://<host>/migrate/todoist
        - Do not put tinyauth in front of /api or CalDAV
        - StateDirectory is /var/lib/vikunja (DynamicUser). Back that up; it is not under Neo appdata

        ## Verification
        - systemctl is-active vikunja
        - curl -fsS http://127.0.0.1:${toString cfg.port}/api/v1/info
        - https://tasks.<domain>/ returns the Vikunja UI (not a tinyauth redirect)
      '';
    };
  };
}
