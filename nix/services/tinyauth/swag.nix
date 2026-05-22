# Tinyauth reverse proxy configuration for SWAG.
{...}: {
  flake.modules.nixos.tinyauth-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.tinyauth;
  in {
    config.neo.services.tinyauth.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app tinyauth;
          set $upstream_port ${toString config.neo.services.tinyauth.port};
          set $upstream_proto http;
          proxy_pass $upstream_proto://$upstream_app:$upstream_port;
        }
      }
    '';
  };
}
