# Pastebin reverse proxy configuration for SWAG.
# Tinyauth is disabled by default (see option.nix).
{...}: {
  flake.modules.nixos.pastebin-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.pastebin;
  in {
    config.neo.services.pastebin.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        client_max_body_size 0;

        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app pastebin;
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
