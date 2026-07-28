# SWAG Dashboard vhost (linuxserver/mods:swag-dashboard + GoAccess).
{...}: {
  flake.modules.nixos.swag-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.swag;
  in {
    config.neo.services.swag.proxyConf = lib.mkDefault ''
      server {
        include /config/nginx/listen-https.conf;
        http2 on;
        server_name ${cfg.subdomain}.*;

        root /dashboard/www;
        index index.php;
        include /config/nginx/ssl.conf;
        client_max_body_size 0;
        include /config/nginx/geo-access.conf;
        location / {
          try_files $uri $uri/ /index.php$is_args$args =404;
          ${lib.neo.authBlock config cfg}
        }

        location ~ ^(.+\.php)(.*)$ {
          fastcgi_split_path_info ^(.+\.php)(.*)$;
          fastcgi_pass 127.0.0.1:9000;
          fastcgi_index index.php;
          include /etc/nginx/fastcgi_params;
          ${lib.neo.authBlock config cfg}
        }

        ${lib.neo.authLocations config cfg}
      }
    '';
  };
}
