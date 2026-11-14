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
      piholeData = cfg.appdata;
      splitDnsActive = lib.neo.splitDnsActive config;
      # NetBird's resolver binds port 53 on a loopback address, which a
      # wildcard publish would take.
      netbirdDnsActive = lib.neo.netbirdDnsActive config;
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
          {
            assertion = !(netbirdDnsActive && cfg.localIP == null);
            message = "neo.services.pihole.localIP must be set when NetBird's \"Use NetBird DNS\" is on, so Pi-hole binds only the LAN address and leaves port 53 on loopback for NetBird's resolver.";
          }
        ];

        systemd.services.docker-pihole.preStart = lib.neo.mkEnsureDirs config [
          piholeData
        ];

        networking.firewall = {
          allowedTCPPorts = [53];
          allowedUDPPorts = [53];
        };

        # The web password reaches the container through an env file rendered at activation.
        neo.core.secrets.containerEnv.pihole.FTLCONF_webserver_api_password = cfg.webPassword;

        virtualisation.oci-containers.containers.pihole = {
          image = cfg.containers.pihole;
          environment =
            {
              TZ = config.neo.core.timeZone;
              FTLCONF_dns_listeningMode = "ALL";
              FTLCONF_dns_upstreams = cfg.upstream;
            }
            // optionalAttrs (cfg.localIP != null) {
              FTLCONF_misc_dnsmasq_lines = dnsMasqLines;
            };
          volumes = [
            "${piholeData}:/etc/pihole"
          ];
          ports = lib.neo.piholeDnsPublishPorts {
            splitDnsActive = splitDnsActive || netbirdDnsActive;
            localIP = cfg.localIP;
          };
          extraOptions = [
            "--health-cmd=dig +short +norecurse +retry=0 @127.0.0.1 pi.hole || exit 1"
            "--health-interval=30s"
            "--health-timeout=5s"
            "--health-retries=3"
          ];
          networks = [(lib.neo.docker.serviceNetwork "pihole")];
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
