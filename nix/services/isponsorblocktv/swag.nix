# iSponsorBlockTV reverse proxy configuration for SWAG (ttyd setup UI).
{...}: {
  flake.modules.nixos.isponsorblocktv-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.isponsorblocktv;
  in {
    config.neo.services.isponsorblocktv.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          proxy_pass http://host.docker.internal:7681;

          ${lib.neo.authBlock config cfg}
          # WebSocket (ttyd): proxy.conf already sets Upgrade + Connection via $connection_upgrade
        }

        ${lib.neo.authLocations config cfg}
      }
    '';
  };
}
