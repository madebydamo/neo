# Hermes skill for paperless.
{...}: {
  flake.modules.nixos.paperless-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.paperless;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.paperless.skill.conf = lib.neo.mkServiceSkill {
      service = "paperless";
      inherit cfg domain;
      description = "Paperless-ngx documents, OCR, API";
      tags = ["neo" "paperless" "documents"];
      title = "Neo · Paperless-ngx";
      body = ''
        ## When to Use
        Document archive, OCR, tags, consume folder, REST API, container health.

        ## Architecture notes
        - API paths may be on publicPaths (tinyauth bypass for API — still need Paperless auth)

        ## CLI extras
        ```bash
        docker exec -it paperless <manage-commands as documented upstream>
        ```

        ## Credentials
        - Neo: `services.paperless.dbPassword` (internal DB only)
        - App users + **API tokens**: create in Paperless UI (Settings → API tokens) — not stored by Neo
        - Edge: tinyauth

        ## Procedures
        1. Health-check containers (derived cheatsheet)
        2. UI login via tinyauth then Paperless
        3. API: use token header; base URL external or internal docker name `paperless`

        ## Pitfalls
        - Do not clear appdata without confirmation — destroys documents DB
        - dbPassword is not the web UI password

        ## Verification
        - UI loads; can search a document; API with token returns 200
      '';
    };
  };
}
