# Hermes skill for immich-drop.
{...}: {
  flake.modules.nixos.immich-drop-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.immich-drop;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.immich-drop.skill.conf = lib.neo.mkServiceSkill {
      service = "immich-drop";
      inherit cfg domain;
      description = "Immich public drop / invite uploads";
      tags = ["neo" "immich" "upload"];
      body = ''
        ## When to Use
        Public invite upload links into Immich albums.

        ## Architecture notes
        - Depends on Immich
        - Often public (auth.available false) by design for invite links

        ## Credentials
        - Admin uses Immich credentials to create invite links
        - Link passwords optional per invite — not stored in Neo

        ## Procedures
        1. Ensure Immich healthy first
        2. Create invites in drop UI
        3. Test anonymous upload link

        ## Pitfalls
        - Public surface — treat invite links as secrets if unrestricted

        ## Verification
        - Upload via invite appears in Immich album
      '';
    };
  };
}
