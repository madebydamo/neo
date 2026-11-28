# Regression: nginx must start even when a proxied hostname is missing from DNS.
# Literal hostnames in proxy_pass and grpc_pass are resolved at config load; NXDOMAIN is [emerg]
# and takes every vhost down. Container names / public names belong in nginx
# variables (request-time DNS via resolver.conf). IPs and host.docker.internal
# (/etc/hosts extra_hosts; resolver does not read hosts) may stay literal.
{...}: {
  perSystem = {
    pkgs,
    lib,
    ...
  }: {
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

    # Regression: a location that includes proxy.conf sets none of its headers
    # again. nginx appends, so the upstream gets the header twice; Home
    # Assistant 2026.11 (aiohttp) answers 400 "Duplicate 'Host' header"
    # (hattori 2026-11-28, swag.proxyPass vhosts). Scans the sources from the
    # include line to the end of its block; swagUpstream asserts the same for
    # its `extra`.
    checks.swag-proxy-conf-duplicates = let
      authz = (import ../../lib/reverseProxy/authorization.nix {inherit lib;}).libExtensions.authorization.neo;
      headers = lib.concatStringsSep "|" (map lib.toLower authz.proxyConfHeaders);
      repeats = authz.proxyConfRepeats;
    in
      assert repeats "proxy_set_header Host $host;" == ["Host"];
      assert repeats "add_header X-Real-IP 1;\nproxy_set_header   x-forwarded-for $x;" == ["X-Forwarded-For"];
      assert repeats "proxy_set_header Remote-User $user;\nproxy_set_header X-Hosted 1;" == [];
        pkgs.runCommand "swag-proxy-conf-duplicates" {nativeBuildInputs = [pkgs.gawk];} ''
          set -euo pipefail
          hits="$(
            find ${../.} ${../../lib} ${../../../templates} -type f \( -name '*.nix' -o -name '*.conf' \) -print0 \
            | xargs -0 awk -v headers='^(${headers})$' '
                FNR == 1 { depth = -1 }
                /^[[:space:]]*#/ { next }
                /^[[:space:]]*include[[:space:]]+\/config\/nginx\/proxy\.conf;[[:space:]]*$/ { depth = 0; next }
                depth >= 0 {
                  if (match(tolower($0), /proxy_set_header[[:space:]]+[^[:space:]]+/)) {
                    h = substr(tolower($0), RSTART, RLENGTH)
                    sub(/proxy_set_header[[:space:]]+/, "", h)
                    if (h ~ headers) print FILENAME ":" FNR ": " $0
                  }
                  depth += gsub(/\{/, "{") - gsub(/\}/, "}")
                  if (depth < 0) depth = -1
                }
              ' || true
          )"
          if [ -n "$hits" ]; then
            echo "ERROR: these locations include /config/nginx/proxy.conf and set its headers again." >&2
            echo "nginx then sends the header twice (strict upstreams answer 400). Drop the line." >&2
            printf '%s\n' "$hits" >&2
            exit 1
          fi
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
      grep -qF -- '--ipv6 --ipv4=false' ${../../modules/core/docker-network.sh}
      touch "$out"
    '';
  };
}
