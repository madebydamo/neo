# SWAG Dashboard vhost (linuxserver/mods:swag-dashboard + GoAccess).
{...}: {
  flake.modules.nixos.swag-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.swag;
  in {
    config.neo.services.swag.proxyConf = lib.mkDefault (lib.neo.mkSwagServer {
      inherit cfg;
      extra = ''
        root /dashboard/www;
        index index.php;
      '';
      body = ''
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
      '';
    });
  };
}
