# RustiCal reverse proxy for SWAG.
# UI protected by tinyauth; /ping and DAV paths bypass via publicPaths.
# When ssoPassword is set, /frontend/login is completed as the tinyauth user
# so the RustiCal password form is never shown.
#
# Auth must stay in the access phase: `return`/`if` in the same location as
# tinyauth-location.conf runs in rewrite and skips auth_request. Use try_files
# (content phase) to jump to a named location after tinyauth succeeds.
# Exception: PROPFIND/OPTIONS/REPORT on `/` return 418 in rewrite on purpose so
# they never hit tinyauth or the SSO 302; GET / stays authenticated.
#
# When Calino is enabled, DAV locations get CORS for the Calino origin.
# OPTIONS is answered here only for that Origin (preflight must not hit
# RustiCal 401). Other clients (DAVx5, GNOME) must reach RustiCal OPTIONS so
# the DAV header is not stripped.
{...}: {
  flake.modules.nixos.rustical-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.rustical;
    tinyauthCfg = config.neo.services.tinyauth or {};
    calinoCfg = config.neo.services.calino or {};
    domain = config.neo.services.swag.domain or null;
    ssoEnabled =
      cfg.auth.enabled
      && (tinyauthCfg.enabled or false)
      && (cfg.ssoPassword or null)
      != null
      && cfg.ssoPassword != "";
    corsOrigin =
      if
        (calinoCfg.enabled or false)
        && (calinoCfg.subdomain or null) != null
        && domain != null
        && domain != ""
      then "https://${calinoCfg.subdomain}.${domain}"
      else null;
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
    corsHeaders = lib.optionalString (corsOrigin != null) ''
      add_header Access-Control-Allow-Origin "${corsOrigin}" always;
      add_header Access-Control-Allow-Methods "GET, PUT, POST, DELETE, PROPFIND, PROPPATCH, REPORT, OPTIONS, MKCOL, MKCALENDAR, COPY, MOVE" always;
      add_header Access-Control-Allow-Headers "Authorization, Content-Type, Depth, Prefer, If-None-Match, If-Match" always;
      add_header Access-Control-Expose-Headers "ETag, DAV, Allow, Location" always;
      add_header Access-Control-Max-Age 86400 always;
    '';
    # Browser preflight from Calino only. DAVx5 OPTIONS has no Origin and
    # needs RustiCal's DAV: calendar-access / addressbook header.
    corsOptionsPreflight = lib.optionalString (corsOrigin != null) ''
      set $rustical_preflight 0;
      if ($request_method = OPTIONS) {
        set $rustical_preflight "${corsOrigin}";
      }
      if ($http_origin != $rustical_preflight) {
        set $rustical_preflight 0;
      }
      if ($rustical_preflight != 0) {
        ${corsHeaders}
        add_header Content-Length 0 always;
        return 204;
      }
    '';
    # DAVx5/GNOME PROPFIND the site root. GET / stays behind tinyauth (and the
    # SSO 302). RustiCal only GET-redirects `/` to `/frontend` (PROPFIND is
    # 405), so send DAV methods to well-known CalDAV (RFC 6764; Apple UA
    # still lands on /caldav-compat).
    davRootSkipAuth = ''
      error_page 418 = @rustical_dav_root;
      if ($request_method ~ ^(PROPFIND|OPTIONS|REPORT)$) {
        return 418;
      }
    '';
    davRootNamed = ''
      location @rustical_dav_root {
        internal;
        return 308 /.well-known/caldav;
      }
    '';
    # RustiCal keeps CalDAV and CardDAV on separate trees. Calino/tsdav looks
    # for CARD:addressbook-home-set on the CalDAV principal (Nextcloud-style).
    # Browser well-known discovery fails (tsdav uses redirect:manual, which is
    # opaque cross-origin). Rewrite the 404 property into a pointer at CardDAV.
    corsCarddavHomeRewrite = lib.optionalString (corsOrigin != null) ''
      location ~ ^/caldav/principal/(?<calino_principal>[^/]+) {
        ${corsOptionsPreflight}
        include /config/nginx/proxy.conf;
        include /config/nginx/resolver.conf;
        set $upstream_app rustical;
        set $upstream_port ${toString cfg.port};
        set $upstream_proto http;
        proxy_set_header X-Forwarded-Port 443;
        proxy_set_header Accept-Encoding "";
        proxy_pass $upstream_proto://$upstream_app:$upstream_port;
        ${corsHeaders}
        sub_filter_types text/xml application/xml;
        sub_filter_once off;
        sub_filter '<addressbook-home-set xmlns="urn:ietf:params:xml:ns:carddav"/>\n            </prop>\n            <status>HTTP/1.1 404 Not Found</status>' '<addressbook-home-set xmlns="urn:ietf:params:xml:ns:carddav"><href>/carddav/principal/$calino_principal/</href></addressbook-home-set>\n            </prop>\n            <status>HTTP/1.1 200 OK</status>';
      }

    '';
    corsDavLocation = lib.optionalString (corsOrigin != null) ''
        # Calino (browser) → RustiCal DAV. No tinyauth: already publicPaths.
      ${corsCarddavHomeRewrite}
        location ~ ^/(caldav|carddav|\.well-known/caldav|\.well-known/carddav|remote\.php/dav) {
          ${corsOptionsPreflight}
      ${proxyCore}
          ${corsHeaders}
        }

    '';
    standardConf = ''
      server {
        include /config/nginx/listen-https.conf;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;
        client_max_body_size 0;
        include /config/nginx/geo-access.conf;

      ${corsDavLocation}
        location = / {
          ${davRootSkipAuth}
      ${proxyCore} ${auth}
        }

      ${davRootNamed}
        location / {
      ${proxyCore} ${auth}
        }

        ${authLoc}
      }
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

      ${corsDavLocation}
        location = / {
          ${davRootSkipAuth}
          ${auth}
          try_files /__rustical_sso_no_such_file @rustical_root;
        }

      ${davRootNamed}

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
        else standardConf
      );
  };
}
