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
          image = "collabora/code";
          autoStart = true;
          environment = {
            domain = nextcloudUrl;
            aliasgroup1 = "https://${nextcloudUrl}:443,https://${builtins.replaceStrings ["."] ["\\\\."] nextcloudUrl}:443";
            server_name = collaboraUrl;
            extra_params = "--o:ssl.enable=false --o:ssl.termination=true";
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
          wantedBy = ["multi-user.target"];
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
            ${occ} app:install richdocuments || true
            ${occ} app:disable richdocuments
            ${occ} app:disable richdocumentscode
            ${occ} app:enable richdocuments
            ${occ} config:app:set richdocuments wopi_url --value 'http://collabora:9980'
            ${occ} config:app:set richdocuments public_wopi_url --value "https://${collaboraUrl}"
            ${occ} config:app:set richdocuments wopi_callback_url --value "https://${nextcloudUrl}"
            ${occ} config:app:set richdocuments wopi_allowlist --value "0.0.0.0/0"
            ${occ} richdocuments:activate-config
            echo "Collabora setup completed."
          '';
        };
      };
    };
}
