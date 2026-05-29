# Neo web UI reverse proxy configuration for SWAG.
# Points to the neo-web systemd service port (default 8081) so https://neo.* serves the config editor.
# Uses tinyauth for authentication via authBlock + authLocations (enabled by default in mkReverseProxyOptions).
{...}: {
  flake.modules.nixos.neo-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.neo;
  in {
    config.neo.services.neo.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;

        client_max_body_size 0;

        location / {
          include /config/nginx/proxy.conf;
          proxy_pass http://host.docker.internal:${toString cfg.port}/;
          ${lib.neo.authBlock config cfg}
        }
      ${lib.neo.authLocations config cfg}
      }
    '';
  };
}
