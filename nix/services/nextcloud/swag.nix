# Nextcloud (and collabora) reverse proxy configuration for SWAG.
# Protects the Nextcloud web UI with tinyauth forward authentication (additional auth for webui).
{...}: {
  flake.modules.nixos.nextcloud-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.nextcloud;
    collaboraSubdomain = config.neo.services.collabora.subdomain;
  in {
    config.neo.services.nextcloud.proxyConf = lib.mkDefault ''
      server {
        include /config/nginx/listen-https.conf;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;
        include /config/nginx/geo-access.conf;
        client_max_body_size 0;

        add_header X-Content-Type-Options "nosniff" always;
        add_header X-Robots-Tag "noindex,nofollow" always;
        add_header Referrer-Policy "strict-origin-when-cross-origin" always;
        add_header Strict-Transport-Security "max-age=15552000; includeSubDomains" always;


        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app nextcloud;
          set $upstream_port ${toString cfg.port};
          set $upstream_proto http;
          proxy_pass $upstream_proto://$upstream_app:$upstream_port;

          proxy_hide_header Referrer-Policy;
          proxy_hide_header X-Content-Type-Options;
          proxy_hide_header X-Frame-Options;
          proxy_hide_header X-XSS-Protection;
          proxy_buffering off;

          ${lib.neo.authBlock config cfg}
        }

        # Support for JavaScript modules (.mjs MIME type)
        location ~* \.(?:mjs|js|css|woff2?|ttf|svg|png|jpg|jpeg|gif|ico)$ {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;

          set $upstream_app nextcloud;
          set $upstream_port 80;
          set $upstream_proto http;

          proxy_pass $upstream_proto://$upstream_app:$upstream_port;

          # Force correct MIME type for ES modules
          default_type application/javascript;

          expires 6M;
          access_log off;
        }

        ${lib.neo.authLocations config cfg}
      }
    '';
  };
}
