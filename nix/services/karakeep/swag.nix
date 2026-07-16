# Karakeep reverse proxy configuration for SWAG.
# Protects the web UI with tinyauth; /api is allowed via publicPaths for extensions/apps.
{...}: {
  flake.modules.nixos.karakeep-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.karakeep;
  in {
    config.neo.services.karakeep.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        client_max_body_size 100M;

        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app karakeep;
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
