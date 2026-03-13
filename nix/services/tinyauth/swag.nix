# Tinyauth reverse proxy configuration for SWAG.
{...}: {
  flake.modules.nixos.tinyauth-swag = {
    config,
    lib,
    ...
  }: {
    config.neo.services.tinyauth.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl http2;
        server_name tinyauth.*;
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
