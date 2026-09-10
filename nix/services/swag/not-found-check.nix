# Unknown-host 404: catch-all default_server + packet-run page.
{...}: {
  perSystem = {pkgs, ...}: {
    checks.swag-unknown-host-404 =
      pkgs.runCommand "swag-unknown-host-404" {
        nativeBuildInputs = [pkgs.nodejs];
      } ''
        set -euo pipefail
        conf=${./not-found/default.conf}
        impl=${./not-found.nix}
        pages=${./not-found/p}
        patcher=${./swag-patcher.sh}
        swag=${./default.nix}

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
        check "error-page-packet" grep -qF 'error_page 404 =404 /p/index.html;' "$conf"
        absent "no-split-clients" grep -qF 'split_clients' "$conf"
        absent "no-proxy-pass" grep -qF 'proxy_pass' "$conf"
        absent "no-tinyauth" grep -qiF 'tinyauth' "$conf"
        check "impl-copies-pages" grep -qF 'www/neo-404' "$impl"
        check "impl-bakes-home" grep -qF '__NEO_HOME__' "$impl"
        check "impl-writes-default-conf" grep -qF 'site-confs/default.conf' "$impl"
        check "impl-strips-tests" grep -qF '*.test.js' "$impl"
        check "swag-still-wipes-site-confs" grep -qF 'rm -r ''${appdataSwag}/nginx/site-confs' "$swag"
        absent "patcher-does-not-clobber-default" grep -qF 'site-confs/default.conf' "$patcher"

        html="$pages/index.html"
        check "page-index-exists" test -f "$html"
        check "page-home-placeholder" grep -qF '__NEO_HOME__' "$html"
        check "page-home-anchor" grep -qF 'href="__NEO_HOME__"' "$html"
        absent "page-no-cdn" grep -qiE 'https?://(cdn|fonts|unpkg|jsdelivr|googleapis)' "$html"
        check "page-reduced-motion" grep -qF 'prefers-reduced-motion' "$html"
        check "page-viewport" grep -qF 'viewport' "$html"
        check "scripts-absolute-p" grep -qF 'src="/p/shared.js"' "$html"
        for js in shared.js course.js render.js engine.js; do
          check "js-$js-exists" test -f "$pages/$js"
          absent "js-$js-no-cdn" grep -qiE 'https?://(cdn|fonts|unpkg|jsdelivr|googleapis)' "$pages/$js"
        done
        absent "no-orbit-page" test -f "$pages/01-orbit.html"
        absent "no-duck-page" test -f "$pages/05-duck.html"

        cp -r "$pages" ./p
        cd p
        node --test course.test.js
        echo "ok course-unit-tests"

        if [ "$fail" -ne 0 ]; then
          exit 1
        fi
        touch "$out"
      '';
  };
}
