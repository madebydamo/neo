# Hermes skill for nextcloud.
{...}: {
  flake.modules.nixos.nextcloud-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.nextcloud;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.nextcloud.skill.conf = lib.neo.mkServiceSkill {
      service = "nextcloud";
      inherit cfg domain;
      description = "Nextcloud files, occ, Collabora, DB";
      tags = ["neo" "nextcloud" "files"];
      body = ''
        ## When to Use
        Files, shares, apps, occ, DB, Collabora office integration.

        ## Architecture notes
        - Collabora: separate service; WOPI integration when both enabled

        ## CLI extras
        ```bash
        docker exec -u www-data -it nextcloud php occ status
        docker exec -u www-data -it nextcloud php occ user:list
        ```

        ## Credentials
        - Neo: `services.nextcloud.dbPassword` (DB)
        - Admin/user accounts: app-managed (first install / occ)
        - Edge: tinyauth; many publicPaths for clients and Collabora

        ## Procedures
        1. Health + `occ status`
        2. Config changes via Neo options then activate; app settings via UI/occ
        3. Collabora: ensure collabora service enabled and WOPI URLs correct

        ## Pitfalls
        - Clearing appdata loses files/DB
        - Client sync needs correct publicPaths / HTTPS

        ## Verification
        - Web UI + occ status OK; upload works
      '';
    };
  };
}
