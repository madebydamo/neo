{
  config,
  lib,
  ...
}: {
  config.neo.services.openclaw.proxyConf = lib.mkDefault ''
    server {
      listen 443 ssl http2;
      server_name openclaw.*;
      include /config/nginx/ssl.conf;

      client_max_body_size 0;

      location / {
        include /config/nginx/proxy.conf;
        include /config/nginx/resolver.conf;
        set $upstream_app openclaw-gateway;
        set $upstream_port 18789;
        set $upstream_proto http;
        proxy_pass $upstream_proto://$upstream_app:$upstream_port;
      }
    }
  '';
}
