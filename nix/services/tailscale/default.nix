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
      # Rendered at activation (secret template); absent without an authKey.
      authKeyFile = "${lib.neo.secretsDir}/rendered/tailscale-auth-key";
    in {
      config = mkIf cfg.enabled (mkMerge [
        {
          services.tailscale.enable = true;

          neo.core.secrets.templates = mkIf (cfg.authKey != null) {
            "tailscale-auth-key" = {
              content = "${cfg.authKey}\n";
              mode = "0600";
            };
          };

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
              # The key used to be written here; it is a secret template now.
              rm -f /etc/tailscale/auth-key
              echo 'if [ -f ${authKeyFile} ] && ! ${checkLoggedIn}; then' > /etc/tailscale/up.sh
              echo '  /run/current-system/sw/bin/tailscale up --auth-key=file:${authKeyFile} ${flagStr}' >> /etc/tailscale/up.sh
              echo 'else' >> /etc/tailscale/up.sh
              echo '  /run/current-system/sw/bin/tailscale up ${flagStr}' >> /etc/tailscale/up.sh
              echo 'fi' >> /etc/tailscale/up.sh
              chmod +x /etc/tailscale/up.sh
            '';
          };

          # Blocking setup (lib.neo.mkSetupService): neo-host-dns and
          # tailscale-split-dns order After= it and need the tailnet up.
          # systemd restarts it on failure (no in-script retry loop).
          systemd.services.tailscale-up = lib.neo.mkSetupService {
            inherit pkgs;
            name = "tailscale-up";
            description = "Configure Tailscale with user settings";
            blocking = true;
            retryInterval = null;
            after = [
              "tailscaled.service"
              "network.target"
            ];
            wantedBy = ["tailscaled.service"];
            script = ''
              /run/current-system/sw/bin/bash /etc/tailscale/up.sh
            '';
            serviceConfig = {
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

          # Wants, not Requires: stopping tailscale-split-dns (it BindsTo
          # tailscaled, and the Tailscale target stops tailscaled) must not
          # take dnsmasq down with it. dnsmasq is host DNS; its zone is under
          # /run and its leases are under /var/lib/dnsmasq, not Neo appdata.
          systemd.services.dnsmasq = {
            after = ["tailscale-split-dns.service"];
            wants = ["tailscale-split-dns.service"];
          };

          # Blocking setup (lib.neo.mkSetupService): Before=dnsmasq so boot
          # writes the zone first. systemd restarts it until tailscale has an IP.
          systemd.services.tailscale-split-dns = lib.neo.mkSetupService {
            inherit pkgs;
            name = "tailscale-split-dns";
            description = "Generate Tailscale split-DNS dnsmasq zone from tailnet IPs";
            blocking = true;
            retryInterval = null;
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
            unitConfig.StartLimitIntervalSec = 0;
            serviceConfig = {
              RestartSec = 5;
              StartLimitIntervalSec = 0;
              # Oneshot default is infinity. A stuck start holds
              # neo-tailscale.target in its activating job.
              TimeoutStartSec = "1min";
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
              # Never block this unit on dnsmasq: we are ordered Before=dnsmasq.
              # listen-address= changes need a full restart; reload does not rebind.
              if systemctl is-active --quiet dnsmasq.service; then
                systemctl --no-block try-restart dnsmasq.service
              fi
            '';
          };
        }))
      ]);
    };
}
