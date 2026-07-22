# Collabora Online service implementation (requires nextcloud).
{...}: {
  flake.modules.nixos.collabora = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfgNextcloud = config.neo.services.nextcloud;
      cfg = config.neo.services.collabora;
      domain = config.neo.services.swag.domain;
      nextcloudUrl = "${cfgNextcloud.subdomain}.${domain}";
      collaboraUrl = "${cfg.subdomain}.${domain}";
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = cfgNextcloud.enabled;
            message = "neo.services.collabora: can only be enabled if neo.services.nextcloud is also enabled.";
          }
        ];

        virtualisation.oci-containers.containers.collabora = {
          image = cfg.containers.collabora;
          autoStart = true;
          # CODE 26.04+ entrypoint is coolwsd --use-env-vars, which ignores
          # extra_params. Pass SSL overrides as cmd so they append to the
          # entrypoint (reverse proxy terminates TLS; container speaks plain HTTP).
          cmd = [
            "--o:ssl.enable=false"
            "--o:ssl.termination=true"
          ];
          environment = {
            domain = nextcloudUrl;
            aliasgroup1 = "https://${nextcloudUrl}:443,https://${builtins.replaceStrings ["."] ["\\\\."] nextcloudUrl}:443";
            server_name = collaboraUrl;
            # Skip self-signed cert generation when SSL is disabled at the app layer.
            DONT_GEN_SSL_CERT = "1";
          };
          capabilities = {
            MKNOD = true;
            SYS_ADMIN = true;
          };
          networks = ["internal"];
        };

        systemd.services.collabora-setup = {
          description = "Nextcloud post-install configuration (collabora)";
          after = [
            "docker-nextcloud-db.service"
            "docker-nextcloud-redis.service"
            "docker-nextcloud.service"
            "docker-collabora.service"
            "nextcloud-setup.service"
          ];
          wants = [
            "docker-nextcloud-db.service"
            "docker-nextcloud-redis.service"
            "docker-nextcloud.service"
            "nextcloud-setup.service"
          ];
          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
            Restart = "on-failure";
            RestartSec = 60;
            StartLimitBurst = 30;
            StartLimitIntervalSec = "30min";
          };
          script = let
            docker = "${pkgs.docker}/bin/docker";
            occ = "${docker} exec --user www-data nextcloud php occ";
          in ''
            echo "Configuring Collabora integration..."
            for _ in $(seq 1 20); do
              if ${occ} app:install richdocuments || true \
                 && ${occ} app:disable richdocuments \
                 && ${occ} app:disable richdocumentscode \
                 && ${occ} app:enable richdocuments \
                 && ${occ} config:app:set richdocuments wopi_url --value 'http://collabora:9980' \
                 && ${occ} config:app:set richdocuments public_wopi_url --value "https://${collaboraUrl}" \
                 && ${occ} config:app:set richdocuments wopi_callback_url --value "https://${nextcloudUrl}" \
                 && ${occ} config:app:set richdocuments wopi_allowlist --value "0.0.0.0/0" \
                 && ${occ} richdocuments:activate-config; then
                echo "Collabora setup completed."
                exit 0
              fi
              echo "collabora setup not ready, retry in 10s"
              sleep 10
            done
            echo "collabora setup gave up after retries (will restart via systemd)"
            exit 1
          '';
        };
        systemd.services.docker-collabora.wants = ["collabora-setup.service"];
      };
    };
}
