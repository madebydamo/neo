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
      piholeData = "${config.neo.core.volumes.appdata}/pihole";
      splitDnsActive = lib.neo.splitDnsActive config;
      names = lib.neo.localDnsNamesFromConfig config;
      dnsMasqLines = concatStringsSep ";" (
        map (n: "address=/${n}/${cfg.localIP}") names
      );
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = !(splitDnsActive && cfg.localIP == null);
            message = "neo.services.pihole.localIP must be set when Tailscale split DNS is enabled, so Pi-hole binds only the LAN address and leaves the Tailscale IP for dnsmasq.";
          }
        ];

        systemd.services.docker-pihole.preStart = lib.neo.mkEnsureDirs config [
          piholeData
        ];

        networking.firewall = {
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
          ports = lib.neo.piholeDnsPublishPorts {
            inherit splitDnsActive;
            localIP = cfg.localIP;
          };
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
