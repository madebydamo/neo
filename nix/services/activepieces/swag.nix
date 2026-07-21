# Activepieces reverse proxy configuration for SWAG.
# UI protected by tinyauth; webhook paths bypass via publicPaths.
# proxy.conf already sets Upgrade/Connection for websockets — do not re-set them.
{...}: {
  flake.modules.nixos.activepieces-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.activepieces;
  in {
    config.neo.services.activepieces.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        client_max_body_size 0;

        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app activepieces;
          set $upstream_port ${toString cfg.port};
          set $upstream_proto http;
          proxy_pass $upstream_proto://$upstream_app:$upstream_port;
          ${lib.neo.authBlock config cfg}
        }

        ${lib.neo.authLocations config cfg}
      }
    '';
  };
}
