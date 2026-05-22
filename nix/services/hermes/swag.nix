# Hermes reverse proxy configuration for SWAG.
# Points to dashboardPort (default 9119) so https://hermes.* serves the web UI.
# Uses tinyauth for authentication via authBlock + authLocations (enabled by default in mkReverseProxyOptions).
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
        listen 443 ssl;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        client_max_body_size 0;

        location / {
          include /config/nginx/proxy.conf;
          proxy_pass http://host.docker.internal:${toString cfg.dashboardPort}/;
          ${lib.neo.authBlock config cfg}
        }
      ${lib.neo.authLocations config cfg}
      }
    '';
  };
}
