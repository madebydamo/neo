# Hermes reverse proxy configuration for SWAG.
{...}: {
  flake.modules.nixos.hermes-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.hermes;
  in {
    config.neo.services.hermes.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl http2;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        client_max_body_size 0;

        location / {
          include /config/nginx/proxy.conf;
          proxy_pass http://host.docker.internal:${toString cfg.gatewayPort}/;
          ${lib.neo.authBlock config cfg}
        }
      ${lib.neo.authLocations config cfg}
      }
    '';
  };
}
