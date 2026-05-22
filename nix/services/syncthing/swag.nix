# Syncthing reverse proxy configuration for SWAG.
{...}: {
  flake.modules.nixos.syncthing-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.syncthing;
  in {
    config.neo.services.syncthing.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        client_max_body_size 0;

        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app syncthing;
          set $upstream_port 8384;
          set $upstream_proto http;
          proxy_pass $upstream_proto://$upstream_app:$upstream_port;

          proxy_hide_header Authorization;
          ${lib.neo.authBlock config cfg}
        }

        location ~ (/syncthing)?/rest {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app syncthing;
          set $upstream_port 8384;
          set $upstream_proto http;
          proxy_pass $upstream_proto://$upstream_app:$upstream_port;

          proxy_hide_header Authorization;
        }
        ${lib.neo.authLocations config cfg}
      }
    '';
  };
}
