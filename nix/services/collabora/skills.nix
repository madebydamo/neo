# Hermes skill for collabora.
{...}: {
  flake.modules.nixos.collabora-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.collabora;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.collabora.skill.conf = lib.neo.mkServiceSkill {
      service = "collabora";
      inherit cfg domain;
      description = "Collabora Online office for Nextcloud";
      tags = ["neo" "collabora" "nextcloud"];
      body = ''
        ## When to Use
        Office editing in browser via Nextcloud/WOPI.

        ## Architecture notes
        - Integrates with Nextcloud richdocuments
        - Often no tinyauth on collabora (WOPI callbacks)
        - Requires nextcloud enabled and configured

        ## Credentials
        - No Neo password; relies on Nextcloud/WOPI

        ## Verification
        - Open a document from Nextcloud in browser editor
      '';
    };
  };
}
