{
  config,
  lib,
  ...
}: {
  config.neo.services.immich.proxyConf = lib.mkDefault ''
    server {
        listen 443 ssl http2;
        listen [::]:443 ssl http2;

        server_name immich.*;

        include /config/nginx/ssl.conf;

        client_max_body_size 0;

        location / {

            include /config/nginx/proxy.conf;
            include /config/nginx/resolver.conf;
            set $upstream_app immich-server;
            set $upstream_port 2283;
            set $upstream_proto http;
            proxy_pass $upstream_proto://$upstream_app:$upstream_port;
        }
    }


  '';
}
