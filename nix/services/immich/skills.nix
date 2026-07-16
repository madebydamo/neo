# Hermes skill for immich.
{...}: {
  flake.modules.nixos.immich-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.immich;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.immich.skill.conf = lib.neo.mkServiceSkill {
      service = "immich";
      inherit cfg domain;
      description = "Immich photos, API keys, libraries";
      tags = ["neo" "immich" "photos"];
      body = ''
        ## When to Use
        Photo library, mobile backup, API keys, machine learning jobs, storage.

        ## Architecture notes
        - Related: immich-drop for public upload links

        ## Credentials
        - Users/API keys: created in Immich UI (Account → API Keys) — not in Neo settings
        - Edge: tinyauth; share/API publicPaths for clients

        ## Procedures
        1. Check all immich containers healthy
        2. API: `x-api-key` header with user-created key
        3. Storage growth: inspect media/appdata volumes

        ## Pitfalls
        - Clearing appdata destroys library metadata; media may be separate
        - Heavy ML jobs need disk/CPU headroom

        ## Verification
        - UI loads; library visible; API key lists albums
      '';
    };
  };
}
