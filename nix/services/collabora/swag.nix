# Nextcloud (and collabora) reverse proxy configuration for SWAG.
# Protects the Nextcloud web UI with tinyauth forward authentication (additional auth for webui).
{...}: {
  flake.modules.nixos.nextcloud-swag = {
    config,
    lib,
    ...
  }: let
    collaboraSubdomain = config.neo.services.collabora.subdomain;
  in {
    config.neo.services.collabora.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl;
        http2 on;
        server_name ${collaboraSubdomain}.*;
        include /config/nginx/ssl.conf;

        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app collabora;
          set $upstream_port 9980;
          set $upstream_proto http;
          proxy_pass $upstream_proto://$upstream_app:$upstream_port;

          proxy_set_header Host $host;
          proxy_set_header Upgrade $http_upgrade;
          proxy_set_header Connection "upgrade";
          proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
          proxy_set_header X-Forwarded-Host $http_host;
          proxy_set_header X-Forwarded-Proto $scheme;
          proxy_set_header X-Real-IP $remote_addr;
        }

        location ^~ /hosting/ {
          proxy_pass http://collabora:9980;
          proxy_set_header Host $host;
          proxy_set_header X-Real-IP $remote_addr;
          proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
          proxy_set_header X-Forwarded-Proto $scheme;
        }

        location ^~ /browser/ {
          proxy_pass http://collabora:9980;
          proxy_set_header Host $host;
          proxy_set_header X-Real-IP $remote_addr;
          proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        }

        location ^~ /cool/ {
          proxy_pass http://collabora:9980;
          proxy_http_version 1.1;
          proxy_set_header Upgrade $http_upgrade;
          proxy_set_header Connection "upgrade";
          proxy_set_header Host $host;
          proxy_set_header X-Real-IP $remote_addr;
          proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
          proxy_set_header X-Forwarded-Proto $scheme;
        }
      }
    '';
  };
}
