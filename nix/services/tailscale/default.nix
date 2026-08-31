# Tailscale service implementation.
{...}: {
  flake.modules.nixos.tailscale = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.tailscale;
    in {
      config = mkIf cfg.enabled (mkMerge [
        {
          services.tailscale.enable = true;
          system.activationScripts.create-tailscale-files = let
            flags =
              []
              ++ (
                if !cfg.acceptDns
                then ["--accept-dns=false"]
                else []
              )
              ++ (
                if cfg.acceptRoutes
                then ["--accept-routes"]
                else []
              )
              ++ (
                if cfg.advertiseExitNode
                then ["--advertise-exit-node"]
                else []
              )
              ++ (
                if cfg.advertiseRoutes != []
                then ["--advertise-routes=${concatStringsSep "," cfg.advertiseRoutes}"]
                else []
              )
              ++ (
                if cfg.exitNode != null
                then ["--exit-node=${cfg.exitNode}"]
                else []
              )
              ++ (
                if cfg.exitNodeAllowLanAccess
                then ["--exit-node-allow-lan-access"]
                else []
              )
              ++ (
                if cfg.hostname != null
                then ["--hostname=${cfg.hostname}"]
                else []
              )
              ++ (
                if cfg.loginServer != "https://controlplane.tailscale.com"
                then ["--login-server=${cfg.loginServer}"]
                else []
              )
              ++ (
                if cfg.ssh
                then ["--ssh"]
                else []
              );
            flagStr = concatStringsSep " " flags;

            checkLoggedIn = "/run/current-system/sw/bin/tailscale status --json 2>/dev/null | /run/current-system/sw/bin/grep -q 'BackendState.*Running'";
          in {
            text = ''
              mkdir -p /etc/tailscale
              ${
                if cfg.authKey != null
                then ''
                  echo '${cfg.authKey}' > /etc/tailscale/auth-key
                  chmod 600 /etc/tailscale/auth-key
                ''
                else ""
              }
              echo 'if [ -f /etc/tailscale/auth-key ] && ! ${checkLoggedIn}; then' > /etc/tailscale/up.sh
              echo '  /run/current-system/sw/bin/tailscale up --auth-key=file:/etc/tailscale/auth-key ${flagStr}' >> /etc/tailscale/up.sh
              echo 'else' >> /etc/tailscale/up.sh
              echo '  /run/current-system/sw/bin/tailscale up ${flagStr}' >> /etc/tailscale/up.sh
              echo 'fi' >> /etc/tailscale/up.sh
              chmod +x /etc/tailscale/up.sh
            '';
          };

          systemd.services.tailscaled.wants = ["tailscale-up.service"];
          systemd.services.tailscale-up = {
            description = "Configure Tailscale with user settings";
            after = [
              "tailscaled.service"
              "network.target"
            ];
            serviceConfig = {
              Type = "oneshot";
              ExecStart = "/run/current-system/sw/bin/bash /etc/tailscale/up.sh";
              RemainAfterExit = true;
              Restart = "on-failure";
              RestartSec = 5;
              StartLimitBurst = 3;
              StartLimitIntervalSec = 300;
            };
          };
        }
        (mkIf cfg.splitDns (let
          names = lib.neo.localDnsNamesFromConfig config;
          namesFile = pkgs.writeText "tailscale-split-dns-names" (
            concatStringsSep "\n" names
            + optionalString (names != []) "\n"
          );
          iface = config.services.tailscale.interfaceName;
        in {
          assertions = [
            {
              assertion = (config.neo.services.swag.domain or null) != null;
              message = "neo.services.tailscale.splitDns requires services.swag.domain (the zone served on the tailnet).";
            }
          ];

          systemd.tmpfiles.rules = [
            "d /run/tailscale-split-dns 0755 root root -"
          ];

          services.dnsmasq = {
            enable = true;
            resolveLocalQueries = false;
            alwaysKeepRunning = true;
            settings = {
              bind-interfaces = true;
              except-interface = "lo";
              no-resolv = true;
              no-hosts = true;
              conf-dir = "/run/tailscale-split-dns";
            };
          };

          networking.firewall.interfaces.${iface} = {
            allowedTCPPorts = [53];
            allowedUDPPorts = [53];
          };

          systemd.services.dnsmasq = {
            after = ["tailscale-split-dns.service"];
            requires = ["tailscale-split-dns.service"];
          };

          systemd.services.tailscale-split-dns = {
            description = "Generate Tailscale split-DNS dnsmasq zone from tailnet IPs";
            startLimitIntervalSec = 0;
            after = [
              "tailscale-up.service"
              "tailscaled.service"
            ];
            wants = ["tailscale-up.service"];
            bindsTo = ["tailscaled.service"];
            before = ["dnsmasq.service"];
            wantedBy = [
              "multi-user.target"
              "tailscaled.service"
            ];
            path = [
              config.services.tailscale.package
              pkgs.coreutils
              pkgs.systemd
            ];
            serviceConfig = {
              Type = "oneshot";
              RemainAfterExit = true;
              Restart = "on-failure";
              RestartSec = 5;
              StartLimitIntervalSec = 0;
            };
            script = ''
              set -euo pipefail
              conf_dir=/run/tailscale-split-dns
              mkdir -p "$conf_dir"
              ipv4="$(tailscale ip -4 2>/dev/null | head -n1 || true)"
              ipv6="$(tailscale ip -6 2>/dev/null | head -n1 || true)"
              if [ -z "$ipv4" ]; then
                echo "tailscale-split-dns: no Tailscale IPv4 yet" >&2
                exit 1
              fi
              {
                echo "listen-address=$ipv4"
                if [ -n "$ipv6" ]; then
                  echo "listen-address=$ipv6"
                fi
                while IFS= read -r name || [ -n "$name" ]; do
                  [ -z "$name" ] && continue
                  echo "address=/$name/$ipv4"
                  if [ -n "$ipv6" ]; then
                    echo "address=/$name/$ipv6"
                  fi
                done < ${namesFile}
              } > "$conf_dir/zone.conf.tmp"
              mv "$conf_dir/zone.conf.tmp" "$conf_dir/zone.conf"
              if systemctl is-active --quiet dnsmasq.service; then
                systemctl restart dnsmasq.service
              fi
            '';
          };
        }))
      ]);
    };
}
