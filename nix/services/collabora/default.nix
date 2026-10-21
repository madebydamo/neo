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
      # WOPI allowlist hosts: primary Nextcloud FQDN plus every customDomain.
      nextcloudHosts =
        unique
        (filter (h: h != null && h != "")
          ([nextcloudUrl] ++ (cfgNextcloud.customDomains or [])));
      # CODE wants a literal host and a regex-escaped form per aliasgroup entry.
      escapeDots = h: builtins.replaceStrings ["."] ["\\\\."] h;
      aliasEntry = h: "https://${h}:443,https://${escapeDots h}:443";
      aliasgroup1 = concatStringsSep "," (map aliasEntry nextcloudHosts);
      # domain env: regex of allowed storage hosts (escaped dots, | -joined).
      domainRegex = concatStringsSep "|" (map escapeDots nextcloudHosts);
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
            domain = domainRegex;
            aliasgroup1 = aliasgroup1;
            server_name = collaboraUrl;
            # Skip self-signed cert generation when SSL is disabled at the app layer.
            DONT_GEN_SSL_CERT = "1";
          };
          # Image HEALTHCHECK runs `coolwsd --probe` (TLS) against the listen
          # port. With ssl.enable=false that is plain HTTP: probe still exits 0
          # but floods logs every ~30s with "Looks like SSL/TLS traffic on plain
          # http port". CODE image has no curl for an HTTP probe alternative.
          extraOptions = [
            "--no-healthcheck"
          ];
          capabilities = {
            MKNOD = true;
            SYS_ADMIN = true;
          };
          networks = ["internal"];
        };

        # Non-blocking setup (lib.neo.mkSetupService): retries every 60s until
        # occ succeeds, then stays active (exited). Re-runs when a nextcloud or
        # collabora container restarts.
        systemd.services.collabora-setup = lib.neo.mkSetupService {
          inherit pkgs;
          name = "collabora-setup";
          description = "Nextcloud post-install configuration (collabora)";
          containers = ["nextcloud-db" "nextcloud-redis" "nextcloud" "collabora"];
          attachTo = ["collabora"];
          after = ["nextcloud-setup.service"];
          wants = ["nextcloud-setup.service"];
          retryInterval = 60;
          script = let
            docker = "${pkgs.docker}/bin/docker";
            occ = "${docker} exec --user www-data nextcloud php occ";
          in ''
            echo "Configuring Collabora integration..."

            # Wait until Nextcloud is out of maintenance / upgrade so occ app
            # commands and richdocuments:* are available (mkSetupService retries).
            status_out="$(${occ} status 2>/dev/null)" || {
              echo "Nextcloud occ status not ready" >&2
              exit 1
            }
            if echo "$status_out" | grep -Eq 'maintenance:[[:space:]]*true'; then
              echo "Nextcloud still in maintenance mode" >&2
              exit 1
            fi
            if echo "$status_out" | grep -Eq 'needsDbUpgrade:[[:space:]]*true'; then
              echo "Nextcloud needs DB upgrade" >&2
              exit 1
            fi

            ${occ} app:install richdocuments || true
            ${occ} app:disable richdocuments
            ${occ} app:disable richdocumentscode
            ${occ} app:enable richdocuments

            ${occ} list 2>/dev/null | grep -q richdocuments || {
              echo "richdocuments occ commands not available yet" >&2
              exit 1
            }

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
