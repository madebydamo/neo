# Immich reverse proxy configuration for SWAG.
{...}: {
  flake.modules.nixos.immich-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.immich;
  in {
    config.neo.services.immich.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        client_max_body_size 0;

        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app immich-server;
          set $upstream_port 2283;
          set $upstream_proto http;
          proxy_pass $upstream_proto://$upstream_app:$upstream_port;
          ${lib.neo.authBlock config cfg}
        }
        ${lib.neo.authLocations config cfg}
      }
    '';
  };
}
