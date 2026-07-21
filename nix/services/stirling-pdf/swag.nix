# Stirling PDF reverse proxy configuration for SWAG.
# Large uploads via client_max_body_size 0. Do not re-set proxy_*_timeout or
# Upgrade/Connection — already provided by include /config/nginx/proxy.conf
# (duplicate proxy_connect_timeout breaks nginx).
{...}: {
  flake.modules.nixos.stirling-pdf-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.stirling-pdf;
  in {
    config.neo.services.stirling-pdf.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        client_max_body_size 0;

        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app stirling-pdf;
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
