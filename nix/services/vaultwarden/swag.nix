# Vaultwarden reverse proxy configuration for SWAG.
# Adds tinyauth forward auth for the web UI (/ and /admin); API and notifications paths are public
# (bypassed via publicPaths and no authBlock) to support Bitwarden clients and websocket.
{...}: {
  flake.modules.nixos.vaultwarden-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.vaultwarden;
  in {
    config.neo.services.vaultwarden.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        client_max_body_size 128M;

        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app vaultwarden;
          set $upstream_port ${toString cfg.port};
          set $upstream_proto http;
          proxy_pass $upstream_proto://$upstream_app:$upstream_port;
          ${lib.neo.authBlock config cfg}
        }

        location ~ ^(/vaultwarden)?/admin {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app vaultwarden;
          set $upstream_port ${toString cfg.port};
          set $upstream_proto http;
          proxy_pass $upstream_proto://$upstream_app:$upstream_port;
          ${lib.neo.authBlock config cfg}
        }

        location ~ (/vaultwarden)?/api {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app vaultwarden;
          set $upstream_port ${toString cfg.port};
          set $upstream_proto http;
          proxy_pass $upstream_proto://$upstream_app:$upstream_port;
        }

        location ~ (/vaultwarden)?/notifications/hub {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app vaultwarden;
          set $upstream_port ${toString cfg.port};
          set $upstream_proto http;
          proxy_pass $upstream_proto://$upstream_app:$upstream_port;
        }

        ${lib.neo.authLocations config cfg}
      }
    '';
  };
}
