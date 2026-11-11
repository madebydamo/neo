# Regression: nginx must start even when a proxied hostname is missing from DNS.
# Literal hostnames in proxy_pass and grpc_pass are resolved at config load; NXDOMAIN is [emerg]
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
          '(proxy|grpc)_pass (https?|grpcs?)://(\$\{|[A-Za-z_])' \
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
      grep -qE 'include[[:space:]]+/config/nginx/ingress-maps\.conf;' "$APPDATA/nginx/nginx.conf"
      grep -qE 'include[[:space:]]+/config/nginx/conf\.d/\*\.conf;' "$APPDATA/nginx/nginx.conf"

      touch "$out"
    '';

    # Regression: the PROXY-protocol port stays on loopback and out of the firewall.
    # nginx takes the visitor address from the PROXY header of any private peer, so
    # an open port lets every device in the LAN choose the address SWAG sees.
    checks.swag-proxy-protocol-loopback = pkgs.runCommand "swag-proxy-protocol-loopback" {} ''
      set -euo pipefail
      module=${./default.nix}

      uses="$(grep -nF 'localHttpsProxyProtocolPort' "$module")"
      [ -n "$uses" ]
      open="$(printf '%s\n' "$uses" | grep -vF '"127.0.0.1:''${toString cfg.localHttpsProxyProtocolPort}:' || true)"
      if [ -n "$open" ]; then
        echo "ERROR: SWAG's PROXY-protocol port may only be published on 127.0.0.1:" >&2
        printf '%s\n' "$open" >&2
        exit 1
      fi

      touch "$out"
    '';

    # Regression: SWAG keeps an IPv6 network. Without one Docker's userland proxy
    # turns every IPv6 visitor into the IPv4 gateway, a "Home network" address.
    checks.swag-ipv6-source = pkgs.runCommand "swag-ipv6-source" {} ''
      set -euo pipefail
      grep -qF -- '"--network=''${lib.neo.swagIpv6Network}"' ${./default.nix}
      grep -qF -- '--ipv6 --ipv4=false' ${../../modules/core/docker-network.nix}
      touch "$out"
    '';
  };
}
