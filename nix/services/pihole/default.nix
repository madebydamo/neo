# Pi-hole service implementation.
{...}: {
  flake.modules.nixos.pihole = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.pihole;
      onlySubdomains = config.neo.services.swag.onlySubdomains;
      piholeData = "${config.neo.volumes.appdata}/pihole";
      appServices =
        filterAttrs (
          n: v: v.enabled && v.subdomain or null != null && n != "swag"
        )
        config.neo.services;
      subdomains = catAttrs "subdomain" (attrValues appServices);
      customDomains = concatLists (catAttrs "customDomains" (attrValues appServices));
      domain = config.neo.services.swag.domain;
      dnsMasqLines = concatStringsSep ";" (
        map (sub: "address=/${sub}.${domain}/${cfg.localIP}") subdomains
        ++ map (cd: "address=/${cd}/${cfg.localIP}") customDomains
        ++ (
          if onlySubdomains
          then []
          else ["address=/${domain}/${cfg.localIP}"]
        )
      );
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-pihole.preStart = lib.concatStringsSep "\n" [
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${piholeData}";
          })
        ];

        networking.firewall = mkIf cfg.enabled {
          allowedTCPPorts = [53];
          allowedUDPPorts = [53];
        };

        virtualisation.oci-containers.containers.pihole = {
          image = "pihole/pihole:latest";
          autoStart = true;
          environment =
            {
              TZ = config.neo.timeZone;
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
            "--network=internal"
            "--health-cmd=dig +short +norecurse +retry=0 @127.0.0.1 pi.hole || exit 1"
            "--health-interval=30s"
            "--health-timeout=5s"
            "--health-retries=3"
          ];
        };
      };
    };
}
