# Streamproxy NixOS container (shares host network so 80/443/2223 bind on host stack).
{...}: {
  flake.modules.nixos.streamproxy-container = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.streamproxy;
      appServices = neo.getProxiedServices config;
      swagCfg = config.neo.services.swag;
      swagEnabled = swagCfg.enabled;
      swagHttp = swagCfg.localHttpPort;
      # PROXY-protocol HTTPS port on local SWAG (plain 443 stays for LAN). Loopback
      # only on the host; streamproxy-swag-pp hands it to the container.
      swagHttpsPP = swagCfg.localHttpsProxyProtocolPort;
      swagDomain = swagCfg.domain;
      streamproxyIp = "192.168.100.11";
      localIp = "192.168.100.10";
      # Names are exact or `*.` wildcards, never regexes, and tunnels win over the
      # local SWAG: see _routes.nix.
      routes = import ./_routes.nix {inherit lib;};
      customDomains = concatLists (catAttrs "customDomains" (attrValues appServices));
      # Local SWAG: PROXY-protocol listener for HTTPS (real client IP for access
      # logs / GoAccess). Rathole backends: TCP is transparent, so PROXY protocol
      # bytes reach remote SWAG :8443/9982. Remote rathole clients must bind HTTPS
      # to SWAG localHttpsProxyProtocolPort (not plain 443).
      rendered = routes.render {
        tunnels =
          mapAttrsToList (name: entry: {
            domains = routes.entryDomains entry;
            https = "127.0.0.1:${toString cfg.ports.${name}.https}";
            http = "127.0.0.1:${toString cfg.ports.${name}.http}";
          })
          cfg.entries;
        swag =
          if swagEnabled
          then {
            domains = customDomains ++ [swagDomain "*.${swagDomain}"];
            https = "${localIp}:${toString swagHttpsPP}";
            http = "${localIp}:${toString swagHttp}";
          }
          else null;
      };
      httpBlock = rendered.httpBlock;
      streamMap = rendered.streamMap;

      nonSwagEntries = filterAttrs (n: _: n != "swag-local") cfg.entries;
      tcpForwardList = attrValues cfg.tcpForwards;
      tcpForwardPorts = map (f: f.listenPort) tcpForwardList;
      tcpForwardRatholeStanzas = flatten (mapAttrsToList (
          _fwdName: fwd: let
            entry = cfg.entries.${fwd.entry};
          in [
            "[server.services.${fwd.entry}_${fwd.serviceSuffix}]"
            "token = \"${entry.token}\""
            "bind_addr = \"0.0.0.0:${toString fwd.listenPort}\""
            ""
          ]
        )
        cfg.tcpForwards);
      # Tokens are secrets: the server config is rendered on the host at
      # activation and bind-mounted read-only into the container.
      ratholeEnabled = nonSwagEntries != {};
      ratholeTemplate = "streamproxy-rathole-server.toml";
      ratholeConfigPath = "/etc/rathole/server.toml";
      configText = ''
        [server]
        bind_addr = "0.0.0.0:2223"

        ${concatStringsSep "\n" (
          flatten (
            mapAttrsToList (name: entry: [
              "[server.services.${name}_http]"
              "token = \"${entry.token}\""
              "bind_addr = \"127.0.0.1:${toString (cfg.ports.${name}.http)}\""
              ""
              "[server.services.${name}_https]"
              "token = \"${entry.token}\""
              "bind_addr = \"127.0.0.1:${toString (cfg.ports.${name}.https)}\""
            ])
            nonSwagEntries
          )
          ++ tcpForwardRatholeStanzas
        )}
      '';

      streamproxyForwarding = port: {
        description = "Forward localhost:${toString port} to streamproxy container";
        after = ["container@streamproxy.service"];
        wants = ["container@streamproxy.service"];
        wantedBy = ["multi-user.target"];

        serviceConfig = {
          ExecStart = "${pkgs.socat}/bin/socat TCP-LISTEN:${toString port},fork,reuseaddr TCP:${streamproxyIp}:${toString port}";
          Restart = "always";
          RestartSec = 2;
          DynamicUser = true;
          AmbientCapabilities = "CAP_NET_BIND_SERVICE";
        };
      };
    in
      mkIf cfg.enabled {
        # A changed file is a new inode, which the bind mount does not follow:
        # restart the container (as before, when the config was in its store path).
        neo.core.secrets.templates.${ratholeTemplate} = mkIf ratholeEnabled {
          content = configText;
          restartUnits = ["container@streamproxy.service"];
        };

        assertions =
          mapAttrsToList (_n: fwd: {
            assertion = builtins.hasAttr fwd.entry cfg.entries;
            message = "neo.services.streamproxy.tcpForwards entry '${fwd.entry}' is not in services.streamproxy.entries";
          })
          cfg.tcpForwards;
        boot.enableContainers = true;
        virtualisation.containers.enable = true;

        networking.firewall.allowedTCPPorts =
          [
            80
            443
            2223
          ]
          ++ tcpForwardPorts;
        systemd.services = {
          streamproxy-local80 = streamproxyForwarding 80;
          streamproxy-local443 = streamproxyForwarding 443;
          # SWAG publishes its PROXY-protocol port on loopback only, which the
          # container cannot reach: pass it on from the container's host address.
          streamproxy-swag-pp = mkIf swagEnabled {
            description = "Forward the streamproxy container to SWAG's PROXY-protocol port";
            after = ["container@streamproxy.service"];
            # The address lives on the container's veth and goes away with it.
            partOf = ["container@streamproxy.service"];
            wantedBy = ["multi-user.target" "container@streamproxy.service"];

            serviceConfig = {
              ExecStart = "${pkgs.socat}/bin/socat TCP-LISTEN:${toString swagHttpsPP},bind=${localIp},fork,reuseaddr TCP:127.0.0.1:${toString swagHttpsPP}";
              Restart = "always";
              RestartSec = 2;
              DynamicUser = true;
            };
          };
        };
        networking.firewall.interfaces."ve-streamproxy".allowedTCPPorts = mkIf swagEnabled [swagHttpsPP];
        neo.exposure.ports =
          {
            streamproxy-http = {
              port = 80;
              label = "Web (HTTP) for this server and the servers behind it";
              service = "streamproxy";
              login = "Each app's own settings (see Apps)";
            };
            streamproxy-https = {
              port = 443;
              label = "Web (HTTPS) for this server and the servers behind it";
              service = "streamproxy";
              login = "Each app's own settings (see Apps)";
            };
            streamproxy-tunnel = {
              port = 2223;
              label = "Tunnel for your other servers";
              service = "streamproxy";
              login = "Each server's tunnel token";
            };
            streamproxy-swag = mkIf swagEnabled {
              port = swagHttpsPP;
              interface = "ve-streamproxy";
              label = "Web traffic handed from the relay to this server";
              service = "streamproxy";
              login = "Only the relay on this server";
            };
          }
          // listToAttrs (map (port:
            nameValuePair "streamproxy-forward-${toString port}" {
              inherit port;
              label = "Forwarded port ${toString port}";
              service = "streamproxy";
              login = "Whatever listens behind the tunnel";
            })
          tcpForwardPorts);

        containers.streamproxy = {
          autoStart = true;
          privateNetwork = true;
          forwardPorts =
            [
              {
                containerPort = 80;
                hostPort = 80;
                protocol = "tcp";
              }
              {
                containerPort = 443;
                hostPort = 443;
                protocol = "tcp";
              }
              {
                containerPort = 2223;
                hostPort = 2223;
                protocol = "tcp";
              }
            ]
            ++ (map (port: {
                containerPort = port;
                hostPort = port;
                protocol = "tcp";
              })
              tcpForwardPorts);
          hostAddress = localIp;
          localAddress = streamproxyIp;

          bindMounts = mkIf ratholeEnabled {
            ${ratholeConfigPath} = {
              hostPath = config.neo.core.secrets.templates.${ratholeTemplate}.path;
              isReadOnly = true;
            };
          };

          config = {pkgs, ...}: {
            networking.firewall.allowedTCPPorts =
              [
                80
                443
                2223
              ]
              ++ tcpForwardPorts;

            services.nginx = {
              enable = true;
              config = ''
                events {
                  worker_connections 768;
                }

                http {
                  sendfile on;
                  tcp_nopush on;
                  types_hash_max_size 2048;

                  default_type application/octet-stream;
                  include ${pkgs.nginx}/conf/mime.types;

                  ssl_protocols TLSv1.2 TLSv1.3;
                  ssl_prefer_server_ciphers on;

                  access_log /var/log/nginx/access.log;

                  gzip on;
                  server_tokens off;
                  add_header X-Content-Type-Options "nosniff" always;
                  add_header X-Frame-Options "SAMEORIGIN" always;
                  add_header Referrer-Policy "strict-origin-when-cross-origin" always;

                  include /etc/nginx/conf.d/*.conf;

                  ${httpBlock}

                  server {
                    listen 80 default_server;
                    server_name _;

                    location = / {
                      return 301 https://github.com/madebydamo/neo;
                    }

                    location / {
                      return 404;
                    }
                  }
                }

                stream {
                  map $ssl_preread_server_name $backend {
                    hostnames;
                    ${streamMap}
                    default 127.0.0.1:1;
                  }

                  server {
                    listen 443 reuseport;
                    ssl_preread on;
                    tcp_nodelay on;

                    proxy_connect_timeout 10s;
                    proxy_timeout 24h;
                    proxy_socket_keepalive on;

                    proxy_protocol on;
                    proxy_pass $backend;
                  }
                }
              '';
            };

            systemd.services.rathole-server = mkIf ratholeEnabled {
              description = "Rathole server";
              after = ["network.target"];
              wantedBy = ["multi-user.target"];
              serviceConfig = {
                # Root-only on the host; the DynamicUser reads it as a credential.
                LoadCredential = "config.toml:${ratholeConfigPath}";
                ExecStart = "${pkgs.rathole}/bin/rathole --server %d/config.toml";
                Restart = "always";
                DynamicUser = true;
              };
            };

            system.stateVersion = "24.11";
          };
        };
      };
}
