# NetBird client: services.netbird from nixpkgs (unit netbird, interface wt0).
#
# Settings that must follow settings.toml go into the module's `config`
# (merged into config.json at every start). Flags of `netbird up` only apply
# at the first login: the login unit runs `up` only while the client is not
# registered.
{...}: {
  flake.modules.nixos.netbird = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.netbird;
      server = config.neo.services.netbird-server or {};
      localServer = server.enabled or false;
      ts = config.neo.services.tailscale;
      client = config.services.netbird.clients.default;
      wg = client.interface;

      managementUrl =
        if cfg.managementUrl != null
        then cfg.managementUrl
        else if localServer
        then lib.neo.publicUrl config server
        else "https://api.netbird.io:443";
      # url.URL as Go marshals it into config.json.
      parsedUrl = let
        m = builtins.match "(https?)://([^/]+)/?" managementUrl;
      in
        if m == null
        then throw "neo.services.netbird.managementUrl: expected https://host[:port], got ${managementUrl}"
        else {
          scheme = elemAt m 0;
          host = let
            h = elemAt m 1;
          in
            if hasInfix ":" h
            then h
            else if elemAt m 0 == "https"
            then "${h}:443"
            else "${h}:80";
        };

      # Fixed loopback address: containers that copied it keep working when
      # the network is re-addressed, and it never collides with a DNS server
      # on the NetBird or Tailscale address.
      resolverAddress = "127.0.0.153";

      domain = config.neo.services.swag.domain or null;
      appNamesToCheck = filter (n: domain != null && hasSuffix ".${domain}" n) (lib.neo.localDnsNamesFromConfig config);

      closedToNetbird = attrNames (filterAttrs (
          _: svc: !(elem "netbird" (svc.ingress or lib.neo.ingressVariants))
        )
        (lib.neo.getProxiedServices config));

      checkScript = ''
        set -euo pipefail
        ip2int() {
          local IFS=.
          set -- $1
          echo $(( ($1 << 24) + ($2 << 16) + ($3 << 8) + $4 ))
        }
        int2ip() {
          echo "$(( ($1 >> 24) & 255 )).$(( ($1 >> 16) & 255 )).$(( ($1 >> 8) & 255 )).$(( $1 & 255 ))"
        }

        cidr=$(ip -4 -o addr show dev ${wg} 2>/dev/null | awk '{print $4}' | head -n1)
        if [ -z "$cidr" ]; then
          echo "netbird-check: ${wg} has no address yet; is the server in the network?" >&2
          exit 1
        fi
        addr=''${cidr%/*}
        bits=''${cidr#*/}
        mask=$(( (0xffffffff << (32 - bits)) & 0xffffffff ))
        actual="$(int2ip $(( $(ip2int "$addr") & mask )))/$bits"

        range=${escapeShellArg cfg.networkRange}
        rbits=''${range#*/}
        rmask=$(( (0xffffffff << (32 - rbits)) & 0xffffffff ))
        if [ $(( $(ip2int "$addr") & rmask )) -ne $(( $(ip2int "''${range%/*}") & rmask )) ]; then
          echo "netbird-check: this server's NetBird address $addr is outside Network address range $range." >&2
          echo "netbird-check: set Network address range to $actual, or the range of the network in its dashboard." >&2
          exit 1
        fi
        echo "netbird-check: address $addr in $range"

        ${optionalString cfg.appNames (
          if !cfg.acceptDns
          then ''
            echo "netbird-check: App names over NetBird not checked here: Use NetBird DNS is off on this server"
          ''
          else if appNamesToCheck == []
          then ''
            echo "netbird-check: no app name under ${toString domain} to check"
          ''
          else ''
            name=${escapeShellArg (head appNamesToCheck)}
            answer=$(dig +short +time=3 +tries=2 @${resolverAddress} "$name" A | tail -n1)
            if [ "$answer" != "$addr" ]; then
              echo "netbird-check: $name resolves to ''${answer:-nothing}, not to this server's NetBird address $addr." >&2
              echo "netbird-check: in the NetBird dashboard, DNS → Zones: add the zone ${toString domain} with records * and @ (A) → $addr." >&2
              exit 1
            fi
            echo "netbird-check: $name → $addr"
          ''
        )}
      '';
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = !(ts.enabled && ts.acceptDns && cfg.acceptDns);
            message = "Use Tailscale DNS (services.tailscale.acceptDns) and Use NetBird DNS (services.netbird.acceptDns) cannot both be on: both take over the server's DNS, and the one that starts last wins. Turn one of them off.";
          }
          {
            assertion = !(ts.enabled && lib.neo.cidr4.overlaps cfg.networkRange "100.64.0.0/10");
            message = "services.netbird.networkRange ${cfg.networkRange} lies in 100.64.0.0/10, which Tailscale uses: Tailscale's firewall then drops NetBird traffic to this server. Move the NetBird network to another range (e.g. 10.213.0.0/16) in its dashboard and here.";
          }
          {
            assertion = !(lib.neo.cidr4.overlaps cfg.networkRange "172.16.0.0/12");
            message = "services.netbird.networkRange ${cfg.networkRange} overlaps the Docker networks (172.16.0.0/12). Move the NetBird network to another range, e.g. 10.213.0.0/16.";
          }
          {
            assertion = !cfg.appNames || domain != null;
            message = "services.netbird.appNames needs services.swag.domain: the zone in the NetBird dashboard is for that domain.";
          }
        ];

        warnings = optional (closedToNetbird != []) "NetBird is on, but these apps are closed to NetBird devices (Reachable from does not list NetBird): ${concatStringsSep ", " closedToNetbird}.";

        neo.core.secrets.templates = mkIf (cfg.setupKey != null) {
          "netbird-setup-key" = {
            content = cfg.setupKey;
            mode = "0600";
          };
        };

        services.netbird = {
          enable = true;
          useRoutingFeatures =
            if cfg.routing && cfg.acceptRoutes
            then "both"
            else if cfg.routing
            then "server"
            else if cfg.acceptRoutes
            then "client"
            else "none";
          clients.default = {
            inherit (cfg) port;
            dns-resolver.address = resolverAddress;
            dir.state = cfg.appdata;
            login = mkIf (cfg.setupKey != null) {
              enable = true;
              setupKeyFile = config.neo.core.secrets.templates."netbird-setup-key".path;
            };
            config = {
              ManagementURL = {
                Scheme = parsedUrl.scheme;
                Host = parsedUrl.host;
              };
              DisableDNS = !cfg.acceptDns;
              DisableClientRoutes = !cfg.acceptRoutes;
              ServerSSHAllowed = cfg.ssh;
            };
          };
        };

        # The identity (WireGuard key, registration) lives in appdata, so
        # snapshots, backup and restore keep the device. Root only: it is a
        # private key.
        systemd.tmpfiles.rules = ["d ${cfg.appdata} 0700 root root -"];

        systemd.services = {
          netbird = {
            # config.d changes reach config.json only at a start.
            restartTriggers = [config.environment.etc."netbird/config.d/50-nixos.json".text];
            serviceConfig.StateDirectory = mkForce "";
            unitConfig.RequiresMountsFor = [cfg.appdata];
          };

          netbird-login = mkIf (cfg.setupKey != null) {
            # Read by `netbird up` (flags from NB_* variables), first login only.
            environment =
              {NB_MANAGEMENT_URL = managementUrl;}
              // optionalAttrs (cfg.hostname != null) {NB_HOSTNAME = cfg.hostname;};
          };

          # Non-blocking: compares what settings.toml says with what the
          # network does, and fails with the fix in its message.
          netbird-check = lib.neo.mkSetupService {
            inherit pkgs;
            name = "netbird-check";
            description = "Check this server's place in the NetBird network";
            after = ["netbird.service"] ++ optional (cfg.setupKey != null) "netbird-login.service";
            wants = ["netbird.service"];
            partOf = ["netbird.service"];
            wantedBy = ["netbird.service"];
            path = [pkgs.iproute2 pkgs.gawk pkgs.dnsutils pkgs.coreutils];
            retryInterval = 15;
            maxAttempts = 20;
            script = checkScript;
          };
        };
      };
    };
}
