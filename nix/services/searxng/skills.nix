# Hermes skill for searxng.
{...}: {
  flake.modules.nixos.searxng-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.searxng;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.searxng.skill.conf = lib.neo.mkServiceSkill {
      service = "searxng";
      inherit cfg domain;
      description = "SearXNG metasearch";
      tags = ["neo" "searxng" "search"];
      body = ''
        ## When to Use
        Private metasearch; engine outages; optional VPN routing.

        ## Credentials
        - Usually none at app level; settings.toml for engines

        ## Procedures
        1. Health-check; test a query
        2. If engines fail, check outbound network/VPN

        ## Verification
        - Search returns results
      '';
    };
  };
}
