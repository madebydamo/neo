# Hermes skill for the example service (published as neo-<name>).
# Describe what the assistant needs to operate the service. Mention only real settings keys.
{...}: {
  neoPlugin.modules.example-skills = {
    config,
    lib,
    plugin,
    ...
  }: let
    name = plugin.name "example";
    cfg = config.neo.services.${name};
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.${name}.skill.conf = lib.neo.mkServiceSkill {
      service = name;
      inherit cfg domain;
      description = "Example static page served by nginx";
      tags = ["neo" "example"];
      title = "Neo · Example${plugin.label}";
      body = ''
        ## When to Use
        Questions about the example page, its greeting text, or container health.

        ## Architecture notes
        - One container: `${name}` (`containers.${name}`, port ${toString cfg.port}) on the internal network
        - Appdata is mounted at `/config`. Neo rewrites `www/index.html` from `services.${name}.greeting` on every start
        - `/health` is on publicPaths (tinyauth bypass). Everything else is behind tinyauth

        ## Procedures
        1. Health-check unit `docker-${name}`
        2. `GET https://<subdomain>.<domain>/health` returns `ok`
        3. To change the page text, set `services.${name}.greeting` and apply

        ## Pitfalls
        - Manual edits to `www/index.html` are overwritten on the next container start

        ## Verification
        - `docker-${name}` is active
        - `GET /health` returns 200 without a tinyauth cookie
        - `GET /` returns 302 to tinyauth
      '';
    };
  };
}
