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
      swagHttps = swagCfg.localHttpsPort;
      swagDomain = swagCfg.domain;
      streamproxyIp = "192.168.100.11";
      localIp = "192.168.100.10";
      httpServerBlock = serverName: proxyPass: ''
        server {
          listen 80;
          server_name ${serverName};

          location / {
            proxy_pass http://${proxyPass};
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
          }
        }
      '';
      entryDomains = name: entry:
        (optional (entry.includeTopLevel) entry.url)
        ++ (optional (entry.wildcard) "*.${entry.url}")
        ++ (entry.customDomains);

      customDomains = concatLists (catAttrs "customDomains" (attrValues appServices));
      allSwagDomains = customDomains ++ [swagDomain "*.${swagDomain}"];

      httpSwagBlock = flatten (optional swagEnabled (map (domain: "${httpServerBlock domain "${localIp}:${toString swagHttp}"}") allSwagDomains));
      httpConfigBlock = flatten (mapAttrsToList (
          name: entry: let
            domains = entryDomains name entry;
            port = cfg.ports.${name}.http;
          in
            map (domain: (httpServerBlock domain "127.0.0.1:${toString port}")) domains
        )
        cfg.entries);

      httpBlock = concatStringsSep "\n" (
        httpSwagBlock ++ httpConfigBlock
      );

      domainLabelCount = domain:
        length (filter (part: part != "") (splitString "." domain));
      moreSpecificFirst = a: b: let
        labelsA = domainLabelCount a.domain;
        labelsB = domainLabelCount b.domain;
        lenA = stringLength a.domain;
        lenB = stringLength b.domain;
      in
        if labelsA != labelsB
        then labelsA > labelsB
        else if lenA != lenB
        then lenA > lenB
        else if a.isWildcard != b.isWildcard
        then !a.isWildcard && b.isWildcard
        else a.domain < b.domain;
      mkStreamMapEntry = domain: isWildcard: line: {
        inherit domain isWildcard line;
      };
      streamSwagEntries = flatten (optional swagEnabled (map (
          domain: let
            isWildcard = hasPrefix "*." domain;
            bare =
              if isWildcard
              then removePrefix "*." domain
              else domain;
          in
            mkStreamMapEntry bare isWildcard "${domain} ${localIp}:${toString swagHttps};"
        )
        allSwagDomains));
      streamConfigEntries = flatten (mapAttrsToList (
          name: entry: let
            httpsPort = cfg.ports.${name}.https;
            backend = "127.0.0.1:${toString httpsPort}";
          in
            (optional entry.wildcard (mkStreamMapEntry entry.url true "~^(?<sub>.+\\.)${escapeRegex entry.url}$ ${backend};"))
            ++ (optional entry.includeTopLevel (mkStreamMapEntry entry.url false "${entry.url} ${backend};"))
            ++ (map (domain: mkStreamMapEntry domain false "${domain} ${backend};") entry.customDomains)
        )
        cfg.entries);
      streamMap = concatStringsSep "\n" (
        map (e: e.line) (sort moreSpecificFirst (streamSwagEntries ++ streamConfigEntries))
      );

      nonSwagEntries = filterAttrs (n: _: n != "swag-local") cfg.entries;
      configFile = pkgs.writeText "rathole-server.toml" ''
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
        boot.enableContainers = true;
        virtualisation.containers.enable = true;

        networking.firewall.allowedTCPPorts = [
          80
          443
          2223
        ];
        systemd.services = {
          streamproxy-local80 = streamproxyForwarding 80;
          streamproxy-local443 = streamproxyForwarding 443;
        };

        containers.streamproxy = {
          autoStart = true;
          privateNetwork = true;
          forwardPorts = [
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
          ];
          hostAddress = localIp;
          localAddress = streamproxyIp;

          config = {pkgs, ...}: {
            networking.firewall.allowedTCPPorts = [
              80
              443
              2223
            ];

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

                    proxy_pass $backend;
                  }
                }
              '';
            };

            systemd.services.rathole-server = mkIf (nonSwagEntries != {}) {
              description = "Rathole server";
              after = ["network.target"];
              wantedBy = ["multi-user.target"];
              serviceConfig = {
                ExecStart = "${pkgs.rathole}/bin/rathole --server ${configFile}";
                Restart = "always";
                DynamicUser = true;
              };
            };

            system.stateVersion = "24.11";
          };
        };
      };
}
