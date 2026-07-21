# Hermes skill for Activepieces.
{...}: {
  flake.modules.nixos.activepieces-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.activepieces;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.activepieces.skill.conf = lib.neo.mkServiceSkill {
      service = "activepieces";
      inherit cfg domain;
      description = "Activepieces flows, webhooks, pieces";
      tags = ["neo" "activepieces" "automation"];
      title = "Neo · Activepieces";
      body = ''
        ## When to Use
        No-code automations, webhook triggers, piece connections, flow runs, queue/worker health.

        ## Architecture notes
        - Containers: `activepieces` (API + worker), `activepieces-db` (Postgres/pgvector), `activepieces-redis`
        - Public URL must match `AP_FRONTEND_URL` (Neo sets `https://<subdomain>.<domain>`) for webhooks and OAuth
        - `/api/v1/webhooks` is on publicPaths (tinyauth bypass) so third parties can POST triggers
        - App login is Activepieces' own user accounts after tinyauth

        ## Credentials
        - Neo: `services.activepieces.encryptionKey` (32 hex chars), `jwtSecret`, `dbPassword` (internal DB only)
        - App users: create on first visit (sign-up) or in platform settings — not stored by Neo
        - Edge: tinyauth

        ## Procedures
        1. Health-check containers (derived cheatsheet)
        2. Open UI via tinyauth, then Activepieces login
        3. Test webhook: create a Catch Webhook flow and POST to the live URL under `/api/v1/webhooks/...`

        ## Pitfalls
        - Rotating `encryptionKey` breaks encrypted piece connections
        - `dbPassword` is not the web UI password
        - Wrong domain / FRONTEND_URL breaks inbound webhooks and app triggers
        - Do not clear appdata without confirmation — destroys flows and DB

        ## Verification
        - Units active; UI loads after tinyauth; webhook POST returns 2xx for a published flow
      '';
    };
  };
}
