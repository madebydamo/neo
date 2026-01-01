{ config, lib, ... }:

{
  config.neo.services.filebrowser.proxyConf = lib.mkDefault ''
    server {
      listen 443 ssl;
      server_name filebrowser.*;
      include /config/nginx/ssl.conf;

      client_max_body_size 0;

      location / {
        include /config/nginx/proxy.conf;
        include /config/nginx/resolver.conf;
        set $upstream_app filebrowser;
        set $upstream_port 80;
        set $upstream_proto http;
        proxy_pass $upstream_proto://$upstream_app:$upstream_port/;

        proxy_max_temp_file_size 2048m;
      }
    }
  '';
}
