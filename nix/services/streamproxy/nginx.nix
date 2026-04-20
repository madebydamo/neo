{...}: {
  flake.modules.nixos.streamproxy-nginx = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.streamproxy;
      swagCfg = config.neo.services.swag;
      swagEnabled = swagCfg.enabled;
      swagHttp = swagCfg.localHttpPort;
      swagHttps = swagCfg.localHttpsPort;
      swagDomain = swagCfg.domain;
      httpSwagBlock = optionalString swagEnabled ''
        server {
          listen 80;
          server_name ${swagDomain} *.${swagDomain};

          location / {
            proxy_pass http://127.0.0.1:${toString swagHttp};
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
          }
        }
      '';
      streamSwagMap = optionalString swagEnabled ''
        ${swagDomain} 127.0.0.1:${toString swagHttps};
        *.${swagDomain} 127.0.0.1:${toString swagHttps};
      '';
    in
      mkIf cfg.enabled {
        networking.firewall.allowedTCPPorts = [
          80
          443
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

              ssl_protocols TLSv1.2 TLSv1.3;
              ssl_prefer_server_ciphers on;

              access_log /var/log/nginx/access.log;

              gzip on;
              server_tokens off;
              add_header X-Content-Type-Options "nosniff" always;
              add_header X-Frame-Options "SAMEORIGIN" always;
              add_header Referrer-Policy "strict-origin-when-cross-origin" always;

              ${concatStringsSep "\n" (
              mapAttrsToList (name: entry: ''
                server {
                  listen 80;
                  server_name ${
                  concatStringsSep " " (
                    optional entry.wildcard "*.${entry.url}" ++ optional entry.includeTopLevel entry.url
                  )
                };

                  location / {
                    proxy_pass http://127.0.0.1:${toString (cfg.ports.${name}.http or 9999)};
                    proxy_set_header Host $host;
                    proxy_set_header X-Real-IP $remote_addr;
                    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
                    proxy_set_header X-Forwarded-Proto $scheme;
                  }
                }
              '')
              cfg.entries
            )}

              ${httpSwagBlock}

              server {
                listen 80 default_server;
                server_name _;

                location / {
                  proxy_pass http://127.0.0.1:9999;
                  proxy_set_header Host $host;
                  proxy_set_header X-Real-IP $remote_addr;
                  proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
                  proxy_set_header X-Forwarded-Proto $scheme;
                }
              }
            }

            stream {
              map $ssl_preread_server_name $backend {
                hostnames;
                ${concatStringsSep "\n" (
              mapAttrsToList (name: entry: ''
                ${optionalString entry.wildcard "~^(?<sub>.+\\.)?${escapeRegex entry.url}$ 127.0.0.1:${
                  toString (cfg.ports.${name}.https)
                };"}
                ${optionalString entry.includeTopLevel "${entry.url} 127.0.0.1:${
                  toString (cfg.ports.${name}.https)
                };"}
              '')
              cfg.entries
            )}
                ${streamSwagMap}
                default 127.0.0.1:9999;
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
      };
}
