# NetBird control plane: netbirdio/netbird-server (management, signal, relay,
# STUN and the built-in login in one process) and the dashboard, behind SWAG
# at one host name (swag.nix). UDP STUN is published directly.
#
# netbird-server-setup runs once (marker in appdata): owner and API token over
# the internal network, network range and IPv6 off, this server joins, the DNS
# zone for its app names, token deleted. Later changes are made in the dashboard.
{...}: {
  flake.modules.nixos.netbird-server = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.netbird-server;
      client = config.neo.services.netbird;
      swag = config.neo.services.swag;
      host = lib.neo.publicHost config cfg;
      url = "https://${toString host}";
      appdata = cfg.appdata;
      configTemplate = "netbird-server-config.yaml";
      ownerTemplate = "netbird-server-owner.json";
      inherit (lib.neo.secrets) jsonString;

      enrol = cfg.enrolThisServer && client.enabled && client.setupKey == null;
      zone = client.enabled && client.appNames && swag.domain != null;
      netbirdCli = getExe config.services.netbird.clients.default.wrapper;

      bootstrap = ''
        set -euo pipefail
        state=${appdata}/bootstrap
        done_marker=${appdata}/bootstrap-done
        if [ -e "$done_marker" ]; then
          echo "netbird-server-setup: already done"
          exit 0
        fi
        umask 077
        mkdir -p "$state"

        server_ip=$(docker inspect -f '{{with index .NetworkSettings.Networks "${lib.neo.docker.serviceNetwork "netbird-server"}"}}{{.IPAddress}}{{end}}' netbird-server 2>/dev/null || true)
        if [ -z "$server_ip" ]; then
          echo "netbird-server-setup: container netbird-server not running yet" >&2
          exit 1
        fi
        base="http://$server_ip:80"
        instance=$(curl -sf -m 10 "$base/api/instance") || {
          echo "netbird-server-setup: server not answering yet" >&2
          exit 1
        }

        if [ "$(echo "$instance" | jq -r .setup_required)" = true ]; then
          curl -sf -m 30 -X POST -H 'Content-Type: application/json' \
            --data-binary @${config.neo.core.secrets.templates.${ownerTemplate}.path} \
            "$base/api/setup" | jq -r .personal_access_token > "$state/token"
          echo "netbird-server-setup: administrator ${toString cfg.ownerEmail} created"
        fi
        if [ ! -s "$state/token" ]; then
          echo "netbird-server-setup: the network was set up without Neo; nothing to do"
          touch "$done_marker"
          exit 0
        fi
        token=$(cat "$state/token")
        api() {
          local method=$1 path=$2
          shift 2
          curl -sf -m 30 -X "$method" -H "Authorization: Token $token" -H 'Content-Type: application/json' "$@" "$base$path"
        }

        # The API replaces the whole settings object: read, change, write back.
        # Range before any device joins; no IPv6 addresses, so devices reach
        # SWAG over IPv4 with their NetBird address.
        account=$(api GET /api/accounts)
        id=$(echo "$account" | jq -r '.[0].id')
        body=$(echo "$account" | jq -c --arg r ${escapeShellArg cfg.networkRange} \
          '{settings: (.[0].settings + {network_range: $r, ipv6_enabled_groups: []}
            | del(.embedded_idp_enabled, .local_auth_disabled, .network_range_v6))}')
        got=$(api PUT "/api/accounts/$id" --data "$body" | jq -r .settings.network_range)
        if [ "$got" != ${escapeShellArg cfg.networkRange} ]; then
          echo "netbird-server-setup: network range is $got, expected ${cfg.networkRange}" >&2
          exit 1
        fi
        echo "netbird-server-setup: network range ${cfg.networkRange}, IPv6 off"

        ${optionalString enrol ''
          # Not a pipe into grep -q: under pipefail its early exit fails the status call.
          status=$(${netbirdCli} status 2>&1 || true)
          if grep -q NeedsLogin <<<"$status"; then
            api POST /api/setup-keys --data '{"name":"neo-this-server","type":"one-off","expires_in":86400,"auto_groups":[],"usage_limit":1,"ephemeral":false}' \
              | jq -r .key > "$state/setup-key"
            ${netbirdCli} up --setup-key-file "$state/setup-key" --management-url ${escapeShellArg url} \
              ${optionalString (client.hostname != null) "--hostname ${escapeShellArg client.hostname}"}
            rm -f "$state/setup-key"
          fi
          for i in $(seq 1 60); do
            addr=$(ip -4 -o addr show dev wt0 2>/dev/null | awk '{print $4}' | cut -d/ -f1 || true)
            [ -n "$addr" ] && break
            sleep 2
          done
          if [ -z "''${addr:-}" ]; then
            echo "netbird-server-setup: this server did not get a NetBird address" >&2
            exit 1
          fi
          echo "netbird-server-setup: this server joined as $addr"
        ''}

        ${optionalString zone ''
          addr=$(ip -4 -o addr show dev wt0 2>/dev/null | awk '{print $4}' | cut -d/ -f1 || true)
          if [ -z "$addr" ]; then
            echo "netbird-server-setup: no NetBird address for the app names zone yet" >&2
            exit 1
          fi
          domain=${escapeShellArg swag.domain}
          if ! api GET /api/dns/zones | jq -e --arg d "$domain" 'any(.[]; .domain == $d)' >/dev/null; then
            all=$(api GET /api/groups | jq -r '.[] | select(.name == "All") | .id')
            zone_id=$(api POST /api/dns/zones --data "$(jq -nc --arg d "$domain" --arg g "$all" \
              '{name: $d, domain: $d, enabled: true, enable_search_domain: false, distribution_groups: [$g]}')" | jq -r .id)
            for name in "*.$domain" "$domain"; do
              api POST "/api/dns/zones/$zone_id/records" --data "$(jq -nc --arg n "$name" --arg a "$addr" \
                '{name: $n, type: "A", content: $a, ttl: 300}')" >/dev/null
            done
            echo "netbird-server-setup: DNS zone $domain → $addr"
          fi
        ''}

        # Nothing long-lived stays behind: delete the administrator's tokens.
        for user in $(api GET /api/users | jq -r '.[] | select(.role == "owner") | .id'); do
          for t in $(api GET "/api/users/$user/tokens" | jq -r '.[].id'); do
            api DELETE "/api/users/$user/tokens/$t" >/dev/null
          done
        done
        rm -f "$state/token"
        touch "$done_marker"
        echo "netbird-server-setup: done"
      '';
    in {
      config = mkIf cfg.enabled {
        assertions =
          lib.neo.requireSecrets "netbird-server" cfg ["ownerPassword" "relaySecret" "dataKey" "sessionKey"]
          ++ [
            {
              assertion = cfg.ownerEmail != null && cfg.ownerEmail != "";
              message = "neo.services.netbird-server: ownerEmail (Administrator e-mail) must be set.";
            }
            {
              assertion = swag.enabled && host != null;
              message = "neo.services.netbird-server needs SWAG with a domain: devices reach the network at https://<subdomain>.<domain>.";
            }
            {
              assertion = !client.enabled || client.networkRange == cfg.networkRange;
              message = "services.netbird.networkRange (${client.networkRange}) must equal services.netbird-server.networkRange (${cfg.networkRange}) on the machine that hosts the network.";
            }
          ];

        warnings =
          optional (config.neo.services.rathole.enabled or false)
          "NetBird server runs behind rathole: its UDP STUN port cannot pass the tunnel, so devices that join will be relayed instead of connecting directly. Run it on a machine with a public address (see the guide \"Run your own private network\").";

        neo.core.secrets.templates = {
          ${configTemplate} = {
            content = ''
              server:
                listenAddress: ":80"
                exposedAddress: "${url}:443"
                stunPorts: [${toString cfg.stunPort}]
                healthcheckAddress: ":9000"
                logLevel: "info"
                logFile: "console"
                authSecret: ${jsonString cfg.relaySecret}
                dataDir: "/var/lib/netbird"
                disableAnonymousMetrics: true
                auth:
                  issuer: "${url}/oauth2"
                  signKeyRefreshEnabled: true
                  sessionCookieEncryptionKey: ${jsonString cfg.sessionKey}
                  dashboardRedirectURIs:
                    - "${url}/nb-auth"
                    - "${url}/nb-silent-auth"
                  cliRedirectURIs:
                    - "http://localhost:53000/"
                reverseProxy:
                  # SWAG, on a Docker network.
                  trustedHTTPProxies: ["172.16.0.0/12"]
                  trustedPeers: ["${cfg.networkRange}"]
                store:
                  engine: "sqlite"
                  encryptionKey: ${jsonString cfg.dataKey}
            '';
            mode = "0400";
            restartUnits = ["docker-netbird-server.service"];
          };
          ${ownerTemplate} = {
            # The password stays out of the store: a placeholder filled at activation.
            content = ''{"email": ${builtins.toJSON cfg.ownerEmail}, "name": "Administrator", "create_pat": true, "pat_expire_in": 1, "password": ${jsonString cfg.ownerPassword}}'';
            mode = "0400";
          };
        };

        networking.firewall.allowedUDPPorts = [cfg.stunPort];

        # No hosts entry for the client on this machine: NetBird's resolver
        # serves /etc/hosts to the containers as well, and 127.0.0.1 is the
        # container itself there. It reaches its own address like any device
        # (a VPS reaches its public address; at home it needs hairpin NAT).

        systemd.services.docker-netbird-server.preStart = lib.neo.mkEnsureDirs config [appdata "${appdata}/data"];
        systemd.tmpfiles.rules = ["d ${appdata}/bootstrap 0700 root root -"];

        virtualisation.oci-containers.containers = {
          netbird-server = {
            image = cfg.containers.netbird-server;
            cmd = ["--config" "/etc/netbird/config.yaml"];
            environment.NB_SETUP_PAT_ENABLED = "true";
            volumes = [
              "${config.neo.core.secrets.templates.${configTemplate}.path}:/etc/netbird/config.yaml:ro"
              "${appdata}/data:/var/lib/netbird"
            ];
            ports = ["${toString cfg.stunPort}:${toString cfg.stunPort}/udp"];
            networks = [(lib.neo.docker.serviceNetwork "netbird-server")];
          };

          netbird-dashboard = {
            image = cfg.containers.netbird-dashboard;
            environment = {
              NETBIRD_MGMT_API_ENDPOINT = url;
              NETBIRD_MGMT_GRPC_API_ENDPOINT = url;
              AUTH_AUDIENCE = "netbird-dashboard";
              AUTH_CLIENT_ID = "netbird-dashboard";
              AUTH_CLIENT_SECRET = "";
              AUTH_AUTHORITY = "${url}/oauth2";
              USE_AUTH0 = "false";
              AUTH_SUPPORTED_SCOPES = "openid profile email groups";
              AUTH_REDIRECT_URI = "/nb-auth";
              AUTH_SILENT_REDIRECT_URI = "/nb-silent-auth";
              NGINX_SSL_PORT = "443";
              LETSENCRYPT_DOMAIN = "none";
            };
            networks = [(lib.neo.docker.serviceNetwork "netbird-server")];
          };
        };

        systemd.services.netbird-server-setup = lib.neo.mkSetupService {
          inherit pkgs;
          name = "netbird-server-setup";
          description = "Create the NetBird network: administrator, address range, this server";
          containers = ["netbird-server"];
          after = optional client.enabled "netbird.service";
          wants = optional client.enabled "netbird.service";
          path = [
            config.virtualisation.docker.package
            pkgs.curl
            pkgs.jq
            pkgs.coreutils
            pkgs.gnugrep
            pkgs.gawk
            pkgs.iproute2
          ];
          retryInterval = 20;
          maxAttempts = 45;
          script = bootstrap;
        };
      };
    };
}
