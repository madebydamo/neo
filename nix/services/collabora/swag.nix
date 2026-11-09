# Collabora reverse proxy configuration for SWAG.
{...}: {
  flake.modules.nixos.collabora-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.collabora;
    upstream = lib.neo.swagUpstream {
      app = "collabora";
      port = 9980;
      proxyConf = false;
    };
  in {
    config.neo.services.collabora.proxyConf = lib.mkDefault (lib.neo.mkSwagServer {
      inherit cfg;
      maxBodySize = null;
      body = ''
        location / {
          ${lib.neo.swagUpstream {
          app = "collabora";
          port = 9980;
        }}
          # proxy.conf already sets Host/Upgrade/Connection/X-Forwarded-*;
          # re-setting Upgrade would send "websocket, websocket" and break WS.
        }

        location ^~ /hosting/ {
          ${upstream}
          proxy_set_header Host $host;
          proxy_set_header X-Real-IP $remote_addr;
          proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
          proxy_set_header X-Forwarded-Proto $scheme;
        }

        location ^~ /browser/ {
          ${upstream}
          proxy_set_header Host $host;
          proxy_set_header X-Real-IP $remote_addr;
          proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        }

        location ^~ /cool/ {
          ${upstream}
          proxy_http_version 1.1;
          proxy_set_header Upgrade $http_upgrade;
          proxy_set_header Connection "upgrade";
          proxy_set_header Host $host;
          proxy_set_header X-Real-IP $remote_addr;
          proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
          proxy_set_header X-Forwarded-Proto $scheme;
        }
      '';
    });
  };
}
