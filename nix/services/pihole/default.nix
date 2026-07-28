# Pi-hole service implementation.
{...}: {
  flake.modules.nixos.pihole = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.pihole;
      onlySubdomains = config.neo.services.swag.onlySubdomains;
      piholeData = "${config.neo.core.volumes.appdata}/pihole";
      swagCfg = config.neo.services.swag;
      appServices = lib.neo.getProxiedServices config;
      subdomains = catAttrs "subdomain" (attrValues appServices);
      customDomains = concatLists (catAttrs "customDomains" (attrValues appServices));
      proxyPassDomains = attrNames (swagCfg.proxyPass or {});
      domain = config.neo.services.swag.domain;
      dnsMasqLines = concatStringsSep ";" (
        map (sub: "address=/${sub}.${domain}/${cfg.localIP}") subdomains
        ++ map (cd: "address=/${cd}/${cfg.localIP}") customDomains
        ++ map (pp: "address=/${pp}/${cfg.localIP}") proxyPassDomains
        ++ (
          if onlySubdomains
          then []
          else ["address=/${domain}/${cfg.localIP}"]
        )
      );
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-pihole.preStart = lib.neo.mkEnsureDirs config [
          piholeData
        ];

        networking.firewall = mkIf cfg.enabled {
          allowedTCPPorts = [53];
          allowedUDPPorts = [53];
        };

        virtualisation.oci-containers.containers.pihole = {
          image = cfg.containers.pihole;
          autoStart = true;
          environment =
            {
              TZ = config.neo.core.timeZone;
              FTLCONF_dns_listeningMode = "ALL";
              FTLCONF_webserver_api_password = cfg.webPassword;
              FTLCONF_dns_upstreams = cfg.upstream;
            }
            // optionalAttrs (cfg.localIP != null) {
              FTLCONF_misc_dnsmasq_lines = dnsMasqLines;
            };
          volumes = [
            "${piholeData}:/etc/pihole"
          ];
          ports = [
            "53:53/tcp"
            "53:53/udp"
          ];
          extraOptions = [
            "--health-cmd=dig +short +norecurse +retry=0 @127.0.0.1 pi.hole || exit 1"
            "--health-interval=30s"
            "--health-timeout=5s"
            "--health-retries=3"
          ];
          networks = ["internal"];
        };

        systemd.services."pihole-update-gravity" = {
          description = "Update Pi-hole gravity lists";
          serviceConfig = {
            Type = "oneshot";
            ExecStart = "${pkgs.docker}/bin/docker exec pihole pihole updateGravity";
          };
        };

        systemd.timers."pihole-update-gravity" = {
          wantedBy = ["timers.target"];
          timerConfig = {
            OnCalendar = "03:00";
            Persistent = true;
          };
        };
      };
    };
}
