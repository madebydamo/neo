# RustiCal reverse proxy for SWAG.
# UI protected by tinyauth; /ping and DAV paths bypass via publicPaths.
# When ssoPassword is set, /frontend/login is completed as the tinyauth user
# so the RustiCal password form is never shown.
#
# Auth must stay in the access phase: `return`/`if` in the same location as
# tinyauth-location.conf runs in rewrite and skips auth_request. Use try_files
# (content phase) to jump to a named location after tinyauth succeeds.
{...}: {
  flake.modules.nixos.rustical-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.rustical;
    tinyauthCfg = config.neo.services.tinyauth or {};
    ssoEnabled =
      cfg.auth.enabled
      && (tinyauthCfg.enabled or false)
      && (cfg.ssoPassword or null)
      != null
      && cfg.ssoPassword != "";
    auth = lib.neo.authBlock config cfg;
    authLoc = lib.neo.authLocations config cfg;
    proxyCore = ''
      include /config/nginx/proxy.conf;
      include /config/nginx/resolver.conf;
      set $upstream_app rustical;
      set $upstream_port ${toString cfg.port};
      set $upstream_proto http;
      proxy_set_header X-Forwarded-Port 443;
      proxy_pass $upstream_proto://$upstream_app:$upstream_port;
    '';
    ssoConf = ''
      server {
        include /config/nginx/listen-https.conf;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;
        client_max_body_size 0;
        include /config/nginx/geo-access.conf;
        # Streamproxy/rathole terminates on SWAG :8443 (PROXY protocol). Nginx
        # would otherwise absolutize redirects as https://host:8443/...
        absolute_redirect off;
        port_in_redirect off;

        location = / {
          ${auth}
          try_files /__rustical_sso_no_such_file @rustical_root;
        }

        location = /frontend {
          ${auth}
          try_files /__rustical_sso_no_such_file @rustical_root;
        }

        location @rustical_root {
          internal;
          return 302 https://$http_host/frontend/login;
        }

        location = /frontend/login {
          ${auth}
          try_files /__rustical_sso_no_such_file @rustical_sso;
        }

        location @rustical_sso {
          internal;
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app rustical;
          set $upstream_port ${toString cfg.port};
          set $upstream_proto http;
          proxy_method POST;
          proxy_set_header Content-Type application/x-www-form-urlencoded;
          proxy_set_header X-Forwarded-Port 443;
          proxy_set_body "username=$user&password=${cfg.ssoPassword}";
          proxy_pass $upstream_proto://$upstream_app:$upstream_port/frontend/login;
        }

        location / {
      ${proxyCore} ${auth}
        }

        ${authLoc}
      }
    '';
  in {
    config.neo.services.rustical.proxyConf =
      lib.mkDefault
      (
        if ssoEnabled
        then ssoConf
        else
          lib.neo.mkSubdomainProxyConf {
            inherit config cfg;
            upstream = "rustical";
            port = cfg.port;
          }
      );
  };
}
