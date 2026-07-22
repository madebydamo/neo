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
        - SWAG terminates TLS; container runs with ssl.enable=false and ssl.termination=true
          (passed as coolwsd cmd args — CODE 26.04+ ignores extra_params under --use-env-vars)
        - Internal WOPI: http://collabora:9980; public: https://collabora.<domain>
        - Often no tinyauth on collabora (WOPI callbacks)
        - Requires nextcloud enabled and configured

        ## Credentials
        - No Neo password; relies on Nextcloud/WOPI

        ## Verification
        - docker logs collabora | grep "SSL support" → disabled + termination enabled
        - curl -sS http://collabora:9980/hosting/discovery (from internal net)
        - Open a document from Nextcloud in browser editor
      '';
    };
  };
}
