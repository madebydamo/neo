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
        - aliasgroup1 / domain include primary nextcloud host and every
          nextcloud customDomains entry (literal + regex-escaped forms)
        - Stock image HEALTHCHECK (coolwsd --probe) is disabled via
          --no-healthcheck: the probe speaks TLS, but Neo listens plain HTTP
          behind SWAG. Leaving the probe causes log spam every ~30s:
          "Looks like SSL/TLS traffic on plain http port". That message means a
          TLS client (bad healthcheck or client using https to the coolwsd port)
          hit plain HTTP — not a SWAG misconfig when upstream_proto is already http.

        ## Credentials
        - No Neo password; relies on Nextcloud/WOPI

        ## Verification
        - docker logs collabora | grep "SSL support" → disabled + termination enabled
        - docker logs collabora should not repeat "SSL/TLS traffic on plain http port"
        - curl -sS http://collabora:9980/hosting/discovery (from internal net)
        - Open a document from Nextcloud (primary and any customDomain) in browser editor
        - collabora-setup active (exited); richdocuments:activate-config OK
      '';
    };
  };
}
