# SWAG Dashboard vhost (linuxserver/mods:swag-dashboard + GoAccess).
# Serves PHP UI from /dashboard/www inside the SWAG container — not a reverse-proxy upstream.
# Uses the same auth helpers as every other service; tinyauth by default via mkReverseProxyOptions.
{...}: {
  flake.modules.nixos.swag-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.swag;
  in {
    config = lib.mkIf cfg.enabled {
      neo.services.swag.proxyConf = lib.mkDefault ''
        ## Neo-managed SWAG Dashboard (swag-dashboard docker-mod)
        # https://github.com/linuxserver/docker-mods/tree/swag-dashboard

        server {
          listen 443 ssl;
          listen [::]:443 ssl;
          http2 on;
          server_name ${cfg.subdomain}.*;

          root /dashboard/www;
          index index.php;
          include /config/nginx/ssl.conf;
          client_max_body_size 0;

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
  };
}
