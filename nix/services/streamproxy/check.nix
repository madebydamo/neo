# Regression: a tunnel below the local SWAG domain gets its own names.
# The tunnel's wildcard was a regex in the stream map, and nginx tries regexes
# only after wildcards, so the local `*.example.com` took every
# `<x>.dev1.example.com` (meilen, 2026-11-23: wrong certificate for
# backup.dev1.syntelis.ch). Checks the rendered names, then runs nginx with them
# and sends real TLS ClientHellos (port 443 path) and Host headers (port 80).
{...}: {
  perSystem = {
    pkgs,
    lib,
    ...
  }: let
    routes = import ./_routes.nix {inherit lib;};
    # Backends: 127.0.0.1:<port>, each answers its own name.
    backends = {
      dev1 = 9101;
      dev2 = 9102;
      other = 9103;
      swag = 9104;
    };
    at = name: "127.0.0.1:${toString backends.${name}}";
    # HTTP backends listen 100 above the stream ones.
    atHttp = name: "127.0.0.1:${toString (backends.${name} + 100)}";
    tunnel = name: entry: {
      domains = routes.entryDomains ({
          wildcard = false;
          includeTopLevel = true;
          customDomains = [];
        }
        // entry);
      https = at name;
      http = atHttp name;
    };
    r = routes.render {
      tunnels = [
        (tunnel "dev1" {
          url = "dev1.example.com";
          wildcard = true;
          # The meilen workaround: the same name again, must stay harmless.
          customDomains = ["*.dev1.example.com"];
        })
        (tunnel "dev2" {
          url = "dev2.example.com";
          wildcard = true;
          includeTopLevel = false;
        })
        # Claims a name the local SWAG also has: the tunnel wins.
        (tunnel "other" {
          url = "shop.example.org";
          customDomains = ["photos.example.com"];
        })
      ];
      swag = {
        domains = ["photos.example.com" "example.com" "*.example.com"];
        https = at "swag";
        http = atHttp "swag";
      };
    };
    lines = lib.splitString "\n" r.streamMap;
    expectTrue = name: cond:
      if cond
      then ""
      else ''
        echo "FAIL ${name}" >&2
        fail=1
      '';
    body = lib.concatStrings [
      (expectTrue "no-regex" (!lib.any (l: lib.hasPrefix "~" l) lines))
      (expectTrue "tunnel-wildcard" (lib.elem "*.dev1.example.com ${at "dev1"};" lines))
      (expectTrue "tunnel-top-level" (lib.elem "dev1.example.com ${at "dev1"};" lines))
      (expectTrue "local-wildcard" (lib.elem "*.example.com ${at "swag"};" lines))
      (expectTrue "names-once" (lib.length lines == lib.length (lib.unique (map (l: lib.head (lib.splitString " " l)) lines))))
      (expectTrue "shared-name-to-tunnel" (lib.elem "photos.example.com ${at "other"};" lines))
    ];

    backendServers = lib.concatStrings (lib.mapAttrsToList (name: port: ''
        server { listen 127.0.0.1:${toString port}; return "${name}\n"; }
      '')
      backends);
    nginxConf = pkgs.writeText "streamproxy-routes-nginx.conf" ''
      daemon on;
      pid /tmp/nginx/nginx.pid;
      error_log /tmp/nginx/error.log;
      events {}
      http {
        access_log off;
        client_body_temp_path /tmp/nginx;
        proxy_temp_path /tmp/nginx;
        fastcgi_temp_path /tmp/nginx;
        uwsgi_temp_path /tmp/nginx;
        scgi_temp_path /tmp/nginx;
        ${lib.replaceStrings ["listen 80;"] ["listen 127.0.0.1:8080;"] r.httpBlock}
        server { listen 127.0.0.1:8080 default_server; return 404; }
        ${lib.concatStrings (lib.mapAttrsToList (name: port: ''
          server { listen 127.0.0.1:${toString (port + 100)}; return 200 "${name}\n"; }
        '')
        backends)}
      }
      stream {
        map $ssl_preread_server_name $backend {
          hostnames;
          ${r.streamMap}
          default 127.0.0.1:1;
        }
        server {
          listen 127.0.0.1:8443;
          ssl_preread on;
          proxy_pass $backend;
        }
        ${backendServers}
      }
    '';
    # Sends a TLS ClientHello for the name and prints what the backend answers.
    hello = pkgs.writeText "hello.py" ''
      import socket, ssl, sys
      ctx = ssl.create_default_context()
      inc, out = ssl.MemoryBIO(), ssl.MemoryBIO()
      tls = ctx.wrap_bio(inc, out, server_hostname=sys.argv[1])
      try:
          tls.do_handshake()
      except ssl.SSLWantReadError:
          pass
      s = socket.create_connection(("127.0.0.1", 8443), timeout=5)
      s.sendall(out.read())
      print(s.recv(100).decode().strip())
    '';
  in {
    checks.streamproxy-routes = pkgs.runCommand "streamproxy-routes" {} ''
      set -euo pipefail
      fail=0
      ${body}
      if [ "$fail" -ne 0 ]; then
        printf '%s\n' ${lib.escapeShellArg r.streamMap} >&2
        exit 1
      fi

      mkdir -p /tmp/nginx
      ${pkgs.nginx}/bin/nginx -e /tmp/nginx/error.log -t -c ${nginxConf} -p /tmp/nginx 2>&1 | tee /tmp/nginx/t.log
      if grep -q 'warn\|emerg' /tmp/nginx/t.log; then
        echo "FAIL nginx warns about the config" >&2
        exit 1
      fi
      ${pkgs.nginx}/bin/nginx -e /tmp/nginx/error.log -c ${nginxConf} -p /tmp/nginx
      for i in $(seq 1 50); do
        ${pkgs.curl}/bin/curl -s -o /dev/null http://127.0.0.1:8080/ && break
        sleep 0.1
      done
      expect() {
        local name=$1 want=$2 got
        got=$(${pkgs.python3}/bin/python3 -I ${hello} "$name" || true)
        [ "$got" = "$want" ] || { echo "FAIL 443 $name: got '$got', want '$want'" >&2; fail=1; }
        got=$(${pkgs.curl}/bin/curl -s -H "Host: $name" http://127.0.0.1:8080/ || true)
        [ "$got" = "$want" ] || { echo "FAIL 80 $name: got '$got', want '$want'" >&2; fail=1; }
      }
      expect backup.dev1.example.com dev1
      expect a.b.dev1.example.com dev1
      expect dev1.example.com dev1
      expect x.dev2.example.com dev2
      expect dev2.example.com swag
      expect photos.example.com other
      expect shop.example.org other
      expect cloud.example.com swag
      expect example.com swag
      kill "$(cat /tmp/nginx/nginx.pid)"
      if [ "$fail" -ne 0 ]; then
        exit 1
      fi
      touch "$out"
    '';
  };
}
