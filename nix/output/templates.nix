{...}: {
  config.flake.templates = {
    homeserver = {
      path = ../../templates/homeserver;
      description = "HSaaS self-replicating homeserver template";
    };
    plugin = {
      path = ../../templates/plugin;
      description = "Neo plugin with an example service";
      welcomeText = ''
        # Neo plugin

        1. `git init && git add -A` (flakes only see tracked files)
        2. Rename `example` under `modules/services/` to your service
        3. `nix fmt && nix flake check`

        Guide: https://github.com/madebydamo/neo/blob/master/docs/PLUGIN-DEVELOPMENT.md
        Coding agents: read AGENTS.md. Claude Code: `/plugin install neo --marketplace madebydamo/neo`
      '';
    };
  };
}
