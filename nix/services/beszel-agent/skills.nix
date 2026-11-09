# Hermes skill for beszel-agent.
{...}: {
  flake.modules.nixos.beszel-agent-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.beszel-agent.skill.conf = lib.neo.mkServiceSkill {
      service = "beszel-agent";
      inherit config;
      description = "Beszel metrics agent to hub";
      tags = ["neo" "beszel" "monitoring"];
      body = ''
        ## When to Use
        Agent not reporting, hub URL/token/key misconfiguration.

        ## Architecture notes
        - No public SWAG UI

        ## Credentials
        - `services.beszel-agent.hubUrl`, `key`, `token` from hub settings

        ## Verification
        - Logs show connected; hub lists this host
      '';
    };
  };
}
