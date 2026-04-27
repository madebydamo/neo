# Paperless reverse proxy configuration for SWAG.
# Protects the web UI with tinyauth forward authentication (additional auth for webui).
{...}: {
  flake.modules.nixos.paperless-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.paperless;
  in {
    config.neo.services.paperless.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl http2;
        listen [::]:443 ssl http2;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        client_max_body_size 0;

        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app paperless;
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
