# Vikunja reverse proxy for SWAG.
# Vikunja is a host systemd unit (pkgs.vikunja), not a container name.
# SWAG runs on the `internal` docker network; host-gateway is that network's
# route to the host, where services.vikunja binds 0.0.0.0:3456.
{...}: {
  flake.modules.nixos.vikunja-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.vikunja;
    auth = lib.neo.authBlock config cfg;
    authLoc = lib.neo.authLocations config cfg;
  in {
    config = lib.mkIf cfg.enabled {
      # host-gateway is resolved by Docker to the internal network's gateway.
      virtualisation.oci-containers.containers.swag.extraOptions = [
        "--add-host=host.docker.internal:host-gateway"
      ];

      neo.services.vikunja.proxyConf = ''
        server {
          include /config/nginx/listen-https.conf;
          http2 on;
          server_name ${cfg.subdomain}.*;
          include /config/nginx/ssl.conf;
          client_max_body_size 0;
          include /config/nginx/geo-access.conf;

          location / {
            include /config/nginx/proxy.conf;
            include /config/nginx/resolver.conf;
            set $upstream_app host.docker.internal;
            set $upstream_port ${toString cfg.port};
            set $upstream_proto http;
            proxy_set_header X-Forwarded-Port 443;
            proxy_set_header X-Forwarded-Proto https;
            proxy_pass $upstream_proto://$upstream_app:$upstream_port;
            ${auth}
          }

          ${authLoc}
        }
      '';
    };
  };
}
