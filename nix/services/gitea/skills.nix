# Hermes skill for Gitea.
{...}: {
  flake.modules.nixos.gitea-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.gitea.skill.conf = lib.neo.mkServiceSkill {
      service = "gitea";
      inherit config;
      description = "Self-hosted Gitea Git forge";
      tags = ["neo" "gitea" "git"];
      title = "Neo · Gitea";
      body = ''
        ## When to Use
        Private Git repos, credential/config forges, org/user management in Gitea.

        ## Credentials
        - Admin created on first install (registration disabled by default)
        - API tokens per user in Gitea settings

        ## Pitfalls
        - ROOT_URL / customDomains must match the public hostname (streamproxy)
        - Tinyauth is off by default so git HTTPS and API tokens work

        ## Verification
        - Open the UI, create a repo, clone over HTTPS
      '';
    };
  };
}
