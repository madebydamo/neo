# Hermes skill for the example service (published as neo-example).
# Describe what the assistant needs to operate the service. Mention only real settings keys.
{...}: {
  flake.modules.nixos.example-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.example;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.example.skill.conf = lib.neo.mkServiceSkill {
      service = "example";
      inherit cfg domain;
      description = "Example static page served by nginx";
      tags = ["neo" "example"];
      title = "Neo · Example";
      body = ''
        ## When to Use
        Questions about the example page, its greeting text, or container health.

        ## Architecture notes
        - One container: `example` (`containers.example`, port ${toString cfg.port}) on the internal network
        - Appdata is mounted at `/config`. Neo rewrites `www/index.html` from `services.example.greeting` on every start
        - `/health` is on publicPaths (tinyauth bypass). Everything else is behind tinyauth

        ## Procedures
        1. Health-check unit `docker-example`
        2. `GET https://<subdomain>.<domain>/health` returns `ok`
        3. To change the page text, set `services.example.greeting` and apply

        ## Pitfalls
        - Manual edits to `www/index.html` are overwritten on the next container start

        ## Verification
        - `docker-example` is active
        - `GET /health` returns 200 without a tinyauth cookie
        - `GET /` returns 302 to tinyauth
      '';
    };
  };
}
