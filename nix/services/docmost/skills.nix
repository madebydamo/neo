# Hermes skill for Docmost.
{...}: {
  flake.modules.nixos.docmost-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.docmost;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.docmost.skill.conf = lib.neo.mkServiceSkill {
      service = "docmost";
      inherit cfg domain;
      description = "Docmost wiki, pages, workspaces";
      tags = ["neo" "docmost" "wiki" "docs"];
      title = "Neo · Docmost";
      body = ''
        ## When to Use
        Collaborative wiki pages, workspaces, real-time editing, knowledge base health.

        ## Architecture notes
        - Containers: `docmost` (app), `docmost-db` (Postgres 18), `docmost-redis` (Redis 8 AOF)
        - Public URL must match `APP_URL` (Neo sets `https://<subdomain>.<domain>`)
        - `/api/health` is on publicPaths (tinyauth bypass) for probes
        - Real-time editor needs WebSockets (SWAG `proxy.conf` already enables Upgrade/Connection)
        - First visit shows workspace setup; that account becomes the workspace owner

        ## Credentials
        - Neo: `services.docmost.appSecret` (32+ chars), `dbPassword` (internal DB only)
        - App users: create on first visit / invites — not stored by Neo
        - Edge: tinyauth

        ## Procedures
        1. Health-check containers (derived cheatsheet)
        2. Open UI via tinyauth, complete workspace setup if first boot
        3. Probe `https://docmost.<domain>/api/health` (publicPaths) for 200

        ## Pitfalls
        - Leaving `appSecret` as the Docker default prevents startup
        - `dbPassword` is not the web UI password
        - Wrong domain / APP_URL breaks links and the editor
        - Do not clear appdata without confirmation — destroys wiki and DB
        - Postgres 18 volume is `/var/lib/postgresql` (not `.../data`)

        ## Verification
        - Units active; `/api/health` returns 200; UI loads after tinyauth (302 to login when unauthenticated)
      '';
    };
  };
}
