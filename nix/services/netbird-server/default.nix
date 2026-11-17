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

      bootstrap = pkgs.writeShellApplication {
        name = "netbird-server-setup";
        runtimeInputs = [
          config.virtualisation.docker.package
          pkgs.curl
          pkgs.jq
          pkgs.coreutils
          pkgs.gnugrep
          pkgs.gawk
          pkgs.iproute2
        ];
        runtimeEnv = {
          NEO_APPDATA = appdata;
          NEO_NETWORK = lib.neo.docker.serviceNetwork "netbird-server";
          NEO_OWNER_FILE = config.neo.core.secrets.templates.${ownerTemplate}.path;
          NEO_OWNER_EMAIL = toString cfg.ownerEmail;
          NEO_RANGE = cfg.networkRange;
          NEO_ENROL = boolToString enrol;
          NEO_ZONE = boolToString zone;
          NEO_DOMAIN = toString swag.domain;
          NEO_URL = url;
          NEO_HOSTNAME = toString client.hostname;
          # Only with a NetBird client on this machine.
          NEO_NETBIRD = optionalString enrol netbirdCli;
        };
        text = builtins.readFile ./setup.sh;
      };
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

        neo.core.containerDirs.netbird-server = [appdata "${appdata}/data"];
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
          };
        };

        systemd.services.netbird-server-setup = lib.neo.mkSetupService {
          inherit pkgs;
          name = "netbird-server-setup";
          description = "Create the NetBird network: administrator, address range, this server";
          containers = ["netbird-server"];
          after = optional client.enabled "netbird.service";
          wants = optional client.enabled "netbird.service";
          retryInterval = 20;
          maxAttempts = 45;
          script = getExe bootstrap;
        };
      };
    };
}
