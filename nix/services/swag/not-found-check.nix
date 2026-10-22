# Unknown-host 404: catch-all default_server + neo 404 page.
{...}: {
  perSystem = {pkgs, ...}: {
    checks.swag-unknown-host-404 = pkgs.runCommand "swag-unknown-host-404" {} ''
      set -euo pipefail
      conf=${./not-found/default.conf}
      impl=${./not-found.nix}
      pages=${./not-found/p}
      patcher=${./swag-patcher.sh}
      swag=${./default.nix}
      listen=${../../lib/reverseProxy/listen.nix}
      ingress=${../../lib/reverseProxy/ingress.nix}
      geo=${../../lib/reverseProxy/geo.nix}

      fail=0
      check() {
        local name="$1"
        shift
        if "$@"; then
          echo "ok $name"
        else
          echo "FAIL $name" >&2
          fail=1
        fi
      }
      absent() {
        local name="$1"
        shift
        if "$@"; then
          echo "FAIL $name" >&2
          fail=1
        else
          echo "ok $name"
        fi
      }

      check "http-default-server" grep -qF 'listen 80 default_server;' "$conf"
      check "https-default-server" grep -qF 'listen 443 ssl default_server;' "$conf"
      check "pp-default-server" grep -qF 'listen __PP_PORT__ ssl proxy_protocol default_server;' "$conf"
      check "server-name-catch-all" grep -qF 'server_name _;' "$conf"
      check "includes-subdomain-vhosts" grep -qF 'include /config/nginx/proxy-confs/*.subdomain.conf;' "$conf"
      check "acme-challenge" grep -qF 'location ^~ /.well-known/acme-challenge/' "$conf"
      check "catchall-includes-error-pages" grep -qF 'include /config/nginx/error-pages.conf;' "$conf"
      absent "catchall-no-legacy-p-prefix" grep -qF 'location ^~ /p/' "$conf"
      check "listen-includes-error-pages" grep -qF 'include /config/nginx/error-pages.conf;' "$listen"
      check "error-page-packet" grep -qF 'error_page 404 =404 /_neo404/index.html;' "$listen"
      check "error-page-asset-prefix" grep -qF 'location ^~ /_neo404/' "$listen"
      check "error-page-alias" grep -qF 'alias /config/www/neo-404/p/;' "$listen"
      check "error-page-ssi" grep -qF 'ssi on;' "$listen"
      absent "default-conf-no-sample-version" grep -qF '## Version' "$conf"
      check "swag-writes-error-pages" grep -qF 'error-pages.conf' "$swag"
      absent "no-proxy-intercept" grep -qF 'proxy_intercept_errors' "$listen" "$conf" "$swag"
      check "ingress-exempts-error-assets" grep -qF '/_neo404/' "$ingress"
      check "geo-exempts-error-assets" grep -qF '/_neo404/' "$geo"
      absent "no-split-clients" grep -qF 'split_clients' "$conf"
      absent "no-proxy-pass" grep -qF 'proxy_pass' "$conf"
      absent "no-tinyauth" grep -qiF 'tinyauth' "$conf"
      check "impl-copies-pages" grep -qF 'www/neo-404' "$impl"
      check "impl-bakes-home" grep -qF '__NEO_HOME__' "$impl"
      check "impl-writes-default-conf" grep -qF 'site-confs/default.conf' "$impl"
      check "swag-still-wipes-site-confs" grep -qF 'rm -r ''${appdataSwag}/nginx/site-confs' "$swag"
      absent "patcher-does-not-clobber-default" grep -qF 'site-confs/default.conf' "$patcher"

      html="$pages/index.html"
      check "page-index-exists" test -f "$html"
      check "page-home-placeholder" grep -qF '__NEO_HOME__' "$html"
      check "page-home-anchor" grep -qF 'href="__NEO_HOME__"' "$html"
      absent "page-no-cdn" grep -qiE 'https?://(cdn|fonts|unpkg|jsdelivr|googleapis)' "$html"
      check "page-reduced-motion" grep -qF 'prefers-reduced-motion' "$html"
      check "page-viewport" grep -qF 'viewport' "$html"
      check "page-kicker-404" grep -qF 'Error 404' "$html"
      check "page-ssi-reference" grep -qF '<!--# echo var="request_id"' "$html"
      check "page-ssi-client-ip" grep -qF '<!--# echo var="remote_addr"' "$html"
      absent "page-ssi-no-raw-encoding" grep -qF 'encoding="none"' "$html"
      check "page-only-index" test "$(find "$pages" -type f | wc -l)" -eq 1

      if [ "$fail" -ne 0 ]; then
        exit 1
      fi
      touch "$out"
    '';
  };
}
