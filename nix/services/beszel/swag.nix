# Beszel reverse proxy for SWAG (tinyauth + WebSocket)
{...}: {
  flake.modules.nixos.beszel-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.beszel;
  in {
    config.neo.services.beszel.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app beszel;
          set $upstream_port 8090;
          set $upstream_proto http;
          proxy_pass $upstream_proto://$upstream_app:$upstream_port;

          ${lib.neo.authBlock config cfg}

          # WebSocket headers only
          proxy_set_header Upgrade $http_upgrade;
          proxy_set_header Connection "upgrade";
        }

        ${lib.neo.authLocations config cfg}
      }
    '';
  };
}
