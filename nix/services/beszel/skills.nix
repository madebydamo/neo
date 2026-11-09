# Hermes skill for beszel.
{...}: {
  flake.modules.nixos.beszel-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.beszel.skill.conf = lib.neo.mkServiceSkill {
      service = "beszel";
      inherit config;
      description = "Beszel monitoring hub UI and API";
      tags = ["neo" "beszel" "monitoring"];
      body = ''
        ## When to Use
        Host/container metrics hub, agents, alerts, hub tokens.

        ## Architecture notes
        - Agents connect via websocket (`publicPaths` often includes `/api`)
        - Pair with beszel-agent service

        ## Credentials
        - Hub users/tokens from hub UI/settings (agent token/key options on agent service)
        - Edge tinyauth for UI; API path may be public for agents

        ## Verification
        - Hub UI shows systems; agent reporting
      '';
    };
  };
}
