# Calino reverse proxy for SWAG.
# UI protected by tinyauth. Upstream Caddy sets X-Frame-Options SAMEORIGIN;
# SWAG proxy.conf hides that header when neo.iframeCookieSupport is on.
#
# /webcal-proxy/<url-encoded-scheme>://<host>/<path> (Calino's proxy URL
# convention) is a 1:1 nginx port of Calino's official CORS proxy
# (upstream proxy/server.mjs):
# - target scheme/host/path parsed from the URI, forwarded with Calino's
#   FORWARDED_HEADERS allowlist (Authorization, Content-Type, Depth, Prefer,
#   If-None-Match, If-Match, Accept*, Origin, Referer, User-Agent); everything
#   else — cookies, tinyauth Remote-*, X-Forwarded-*, Sec-Fetch-*, client IP —
#   is stripped
# - CORS headers incl. WebDAV methods and Expose-Headers ETag/Location/
#   X-Target-URL on every response; OPTIONS answers 204 without upstream
# - X-Target-URL echoes the resolved target (Calino reads it for discovery);
#   upstream redirects are relayed untouched (redirect: 'manual' parity)
# - body cap 10m and 30s upstream timeout mirror MAX_BODY_BYTES/FETCH_TIMEOUT_MS
# Deliberate divergences (documented, not bugs): ALLOWED_ORIGINS is fixed to
# this Calino instance but a missing Origin is accepted (same-origin GETs may
# omit it; browsers do not send it on same-origin navigational requests) —
# tinyauth in front is the abuse boundary. Any target host and scheme is
# allowed (ALLOWED_TARGETS = *) and the private-IP denylist of server.mjs is
# not reproduced; nginx cannot range-check resolved upstreams.
# NOTE: proxy.conf is intentionally NOT included here — its proxy_set_header
# entries would be duplicated (not overridden) by ours and several fronting
# servers answer that with bare "400 Bad Request".
# After nginx decodes the path, merge_slashes turns https://host into
# https:/host — :/+ accepts both; // inside the target path collapses too.
{...}: {
  flake.modules.nixos.calino-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.calino;
    auth = lib.neo.authBlock config cfg;
    authLoc = lib.neo.authLocations config cfg;
    calinoOrigin = "https://${cfg.subdomain}.${config.neo.services.swag.domain}";
  in {
    config.neo.services.calino.proxyConf = lib.mkDefault ''
      # ALLOWED_ORIGINS = [ ${calinoOrigin} ]; empty Origin tolerated.
      map $http_origin $webcal_origin_allowed {
        default 0;
        "" 1;
        "${calinoOrigin}" 1;
      }
      map $http_origin $webcal_cors_origin {
        default "";
        "${calinoOrigin}" $http_origin;
      }

      server {
        include /config/nginx/listen-https.conf;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;
        client_max_body_size 0;
        include /config/nginx/geo-access.conf;

        location ~ ^/webcal-proxy/(?<webcal_scheme>https?):/+(?<webcal_host>[^/?#]+)(?<webcal_path>/.*)?$ {
          include /config/nginx/resolver.conf;
          ${auth}

          # Origin gate (403 without CORS headers, per server.mjs).
          if ($webcal_origin_allowed = 0) {
            return 403;
          }
          # CORS preflight: answer locally, never touch the upstream.
          if ($request_method = OPTIONS) {
            return 204;
          }

          add_header Access-Control-Allow-Origin $webcal_cors_origin always;
          add_header Access-Control-Allow-Methods "GET, POST, PUT, DELETE, PROPFIND, PROPPATCH, REPORT, OPTIONS, MKCOL, MKCALENDAR, COPY, MOVE" always;
          add_header Access-Control-Allow-Headers "Authorization, Content-Type, Depth, Prefer, If-None-Match, If-Match, X-Follow-Redirects" always;
          add_header Access-Control-Expose-Headers "ETag, Location, X-Target-URL" always;
          add_header X-Target-URL "$webcal_scheme://$webcal_host$webcal_path$is_args$args" always;

          # MAX_BODY_BYTES (10 MiB) and FETCH_TIMEOUT_MS (30 s).
          client_max_body_size 10m;
          proxy_connect_timeout 30s;
          proxy_send_timeout 30s;
          proxy_read_timeout 30s;

          proxy_http_version 1.1;
          proxy_ssl_server_name on;
          proxy_ssl_name $webcal_host;
          proxy_set_header Host $webcal_host;
          # FORWARDED_HEADERS allowlist: strip every credential/metadata hop
          # header the browser or tinyauth injected.
          proxy_set_header Connection "";
          proxy_set_header Cookie "";
          proxy_set_header Early-Data "";
          proxy_set_header Proxy "";
          proxy_set_header Upgrade "";
          proxy_set_header X-Forwarded-For "";
          proxy_set_header X-Forwarded-Host "";
          proxy_set_header X-Forwarded-Method "";
          proxy_set_header X-Forwarded-Port "";
          proxy_set_header X-Forwarded-Proto "";
          proxy_set_header X-Forwarded-Server "";
          proxy_set_header X-Forwarded-Ssl "";
          proxy_set_header X-Forwarded-Uri "";
          proxy_set_header X-Original-Method "";
          proxy_set_header X-Original-URL "";
          proxy_set_header X-Real-IP "";
          proxy_set_header Remote-Email "";
          proxy_set_header Remote-Groups "";
          proxy_set_header Remote-Name "";
          proxy_set_header Remote-User "";
          proxy_set_header DNT "";
          proxy_set_header Sec-GPC "";
          proxy_set_header Sec-Fetch-Dest "";
          proxy_set_header Sec-Fetch-Mode "";
          proxy_set_header Sec-Fetch-Site "";
          proxy_set_header Sec-Fetch-User "";

          proxy_pass $webcal_scheme://$webcal_host$webcal_path$is_args$args;
        }

        location / {
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app calino;
          set $upstream_port ${toString cfg.port};
          set $upstream_proto http;
          proxy_pass $upstream_proto://$upstream_app:$upstream_port;
          ${auth}
        }

        ${authLoc}
      }
    '';
  };
}
