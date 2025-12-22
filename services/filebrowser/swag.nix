{ config, lib, ... }:

{
  config.neo.services.filebrowser.proxyConf = lib.mkDefault ''
    server {
      listen 443 ssl http2;
      server_name filebrowser.*;
      include /config/nginx/ssl.conf;

      client_max_body_size 0;

      location / {
        include /config/nginx/proxy.conf;
        resolver 127.0.0.11 valid=30s;
        set $upstream_app filebrowser;
        set $upstream_port 80;
        set $upstream_proto http;
        proxy_pass $upstream_proto://$upstream_app:$upstream_port/;

        proxy_max_temp_file_size 2048m;
      }
    }
  '';
}
