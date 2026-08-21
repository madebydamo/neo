# Regression: nginx must start even when a proxied hostname is missing from DNS.
# Literal hostnames in proxy_pass are resolved at config load; NXDOMAIN is [emerg]
# and takes every vhost down. Container names / public names belong in nginx
# variables (request-time DNS via resolver.conf). IPs and host.docker.internal
# (/etc/hosts extra_hosts; resolver does not read hosts) may stay literal.
{...}: {
  perSystem = {pkgs, ...}: {
    checks.swag-deferred-proxy-pass = pkgs.runCommand "swag-deferred-proxy-pass" {} ''
      set -euo pipefail
      services=${../.}
      lib=${../../lib}
      templates=${../../../templates}
      patcher=${./swag-patcher.sh}

      hits="$(
        grep -RInE --include='*.nix' --include='*.conf' \
          'proxy_pass https?://(\$\{|[A-Za-z_])' \
          "$services" "$lib" "$templates" \
        | grep -E '(/swag\.nix:|/swag/|/reverseProxy/|/templates/plugin/)' \
        | grep -v 'host\.docker\.internal' \
        | grep -v '/skills\.nix:' \
        || true
      )"
      extra="$(
        grep -RInE --include='*.nix' \
          'proxy_pass \$\{' \
          "$services/swag" "$lib/reverseProxy" \
        | grep -v 'proxy_pass ''${proxyPass}' \
        || true
      )"

      if [ -n "$hits" ] || [ -n "$extra" ]; then
        echo "ERROR: hostname proxy_pass is resolved at nginx start; a missing backend takes SWAG down." >&2
        echo "Use set \$upstream_* + proxy_pass \$upstream_proto://\$upstream_app:\$upstream_port" >&2
        echo "or proxy_pass https://127.0.0.1 (custom-domain hairpin)." >&2
        printf '%s\n%s\n' "$hits" "$extra" >&2
        exit 1
      fi

      export APPDATA="$TMPDIR/swag"
      export NEO_UID="$(id -u)"
      export NEO_GID="$(id -g)"
      export NEO_SUPPORT=false
      export DASHBOARD_SUBDOMAIN=swag
      mkdir -p "$APPDATA/nginx/proxy-confs" "$APPDATA/nginx/conf.d"
      cat > "$APPDATA/nginx/nginx.conf" <<'EOF'
      http {
          include /config/nginx/resolver.conf;
          server { listen 80; }
      }
      EOF
      touch "$APPDATA/nginx/proxy.conf"
      bash "$patcher"
      grep -qE 'include[[:space:]]+/config/nginx/dbip\.conf;' "$APPDATA/nginx/nginx.conf"
      grep -qE 'include[[:space:]]+/config/nginx/conf\.d/\*\.conf;' "$APPDATA/nginx/nginx.conf"

      touch "$out"
    '';
  };
}
