# OpenClaw reverse proxy configuration for SWAG.
{...}: {
  flake.modules.nixos.openclaw-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.openclaw;
    domain = config.neo.services.swag.domain;
    tinyauthCfg = config.neo.services.tinyauth;
    authEnabled = cfg.auth.enabled && tinyauthCfg.enabled;
    authBlock = ''

      # Tinyauth forward authentication
      auth_request /tinyauth;
      error_page 401 = @tinyauth_login;
    '';
    authLocations = ''

      # Tinyauth auth request handler
      location /tinyauth {
        internal;
        proxy_pass http://tinyauth:${toString tinyauthCfg.port}/api/auth/nginx;
        proxy_set_header x-forwarded-proto $scheme;
        proxy_set_header x-forwarded-host $http_host;
        proxy_set_header x-forwarded-uri $request_uri;
      }

      # Tinyauth login redirect
      location @tinyauth_login {
        return 302 https://${tinyauthCfg.subdomain}.${domain}/login?redirect_uri=$scheme://$http_host$request_uri;
      }
    '';
  in {
    config.neo.services.openclaw.proxyConf = lib.mkDefault ''
      server {
        listen 443 ssl http2;
        server_name openclaw.*;
        include /config/nginx/ssl.conf;

        client_max_body_size 0;

        location / {
          include /config/nginx/proxy.conf;
          proxy_pass http://host.docker.internal:${toString cfg.gatewayPort}/;
          ${lib.optionalString authEnabled authBlock}
        }
        ${lib.optionalString authEnabled authLocations}
      }
    '';
  };
}
