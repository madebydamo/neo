# Hermes skill for streamproxy.
{...}: {
  flake.modules.nixos.streamproxy-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.streamproxy;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.streamproxy.skill.conf = lib.neo.mkServiceSkill {
      service = "streamproxy";
      inherit cfg domain;
      description = "Public edge: nginx + rathole server multi-tenant";
      tags = ["neo" "streamproxy" "edge"];
      body = ''
        ## When to Use
        Public VPS edge routing multiple homeservers via rathole.

        ## Architecture notes
        - Host nginx stream + rathole server
        - Entries map domains/tokens to tunnel ports

        ## Credentials
        - Per-entry tokens in `services.streamproxy` entries

        ## Verification
        - Remote rathole clients connected; domain TLS works
      '';
    };
  };
}
