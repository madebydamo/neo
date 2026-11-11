# Unit checks for per-service ingress maps and SWAG wiring.
{...}: {
  perSystem = {
    pkgs,
    lib,
    ...
  }: let
    ingress = (import ./ingress.nix {inherit lib;}).libExtensions.ingress.neo;

    expectTrue = name: cond:
      if cond
      then ""
      else ''
        echo "FAIL ${name}" >&2
        fail=1
      '';

    localOnly = {
      pastebin = {
        subdomain = "pastebin";
        customDomains = ["pastes.example.com"];
        ingress = ["local"];
      };
    };
    allThree = {
      neo = {
        subdomain = "neo";
        customDomains = [];
        ingress = ["local" "tailscale" "netbird" "web"];
      };
    };
    webAndTailscale = {
      vault = {
        subdomain = "vaultwarden";
        ingress = ["tailscale" "web"];
      };
    };
    mixed = localOnly // allThree // webAndTailscale;

    localConf = ingress.mkIngressMapsConf {services = localOnly;};
    allConf = ingress.mkIngressMapsConf {services = allThree;};
    mixedConf = ingress.mkIngressMapsConf {services = mixed;};
    altPortConf = ingress.mkIngressMapsConf {
      services = allThree;
      proxyProtocolPort = 9982;
    };
    noNetbird = {
      jelly = {
        subdomain = "jelly";
        ingress = ["local" "tailscale" "web"];
      };
    };
    netbirdConf = ingress.mkIngressMapsConf {
      services = noNetbird;
      netbirdRanges = ["10.213.0.0/16"];
    };

    # nginx itself classifies: X-Real-IP stands in for the visitor address,
    # port 8443 for the PROXY-protocol listener.
    classifyConf = pkgs.writeText "classify.conf" ''
      pid /tmp/nginx/nginx.pid;
      error_log stderr;
      events {}
      http {
        access_log off;
        ${ingress.mkIngressMapsConf {
        services = noNetbird;
        netbirdRanges = ["10.213.0.0/16" "100.70.0.0/16"];
      }}
        set_real_ip_from 127.0.0.1;
        real_ip_header X-Real-IP;
        client_body_temp_path /tmp/nginx/body;
        proxy_temp_path /tmp/nginx/proxy;
        fastcgi_temp_path /tmp/nginx/fastcgi;
        uwsgi_temp_path /tmp/nginx/uwsgi;
        scgi_temp_path /tmp/nginx/scgi;
        server {
          listen 127.0.0.1:8080;
          listen 127.0.0.1:8443;
          location / {
            return 200 "$ingress_class\n";
          }
        }
      }
    '';

    body = lib.concatStrings [
      (expectTrue "variants" (ingress.ingressVariants == ["local" "tailscale" "netbird" "web"]))
      (expectTrue "access-class-netbird" (lib.hasInfix "$ingress_allow_netbird" ingress.ingressAccessConf))
      (expectTrue "netbird-geo-range" (lib.hasInfix "10.213.0.0/16 1;" netbirdConf))
      (expectTrue "netbird-geo-empty-without-client" (!lib.hasInfix "10.213.0.0/16" localConf))
      (expectTrue "netbird-not-ticked-denies" (lib.hasInfix "~*^jelly(\\.|$) 0;" netbirdConf))
      (expectTrue "access-returns-404" (lib.hasInfix "return 404;" ingress.ingressAccessConf))
      (expectTrue "access-exempts-error-assets" (lib.hasInfix "/_neo404/" ingress.ingressAccessConf))
      (expectTrue "access-class-web" (lib.hasInfix "$ingress_allow_web" ingress.ingressAccessConf))
      (expectTrue "pp-port-default" (lib.hasInfix "8443 1;" localConf))
      (expectTrue "pp-port-override" (lib.hasInfix "9982 1;" altPortConf))
      (expectTrue "tailscale-cgnat" (lib.hasInfix "100.64.0.0/10 1;" localConf))
      (expectTrue "local-only-denies-web-sub" (lib.hasInfix "~*^pastebin(\\.|$) 0;" localConf))
      (expectTrue "local-only-denies-web-custom" (lib.hasInfix "pastes.example.com 0;" localConf))
      (expectTrue "all-three-no-host-deny" (!lib.hasInfix "~*^neo(\\.|$)" allConf))
      (expectTrue "mixed-vault-denies-local" (
        lib.hasInfix "~*^vaultwarden(\\.|$) 0;" mixedConf
        && lib.hasInfix "~*^pastebin(\\.|$) 0;" mixedConf
      ))
    ];
  in {
    checks.ingress-posture = pkgs.runCommand "ingress-posture" {} ''
      set -euo pipefail
      fail=0
      ${body}
      if [ "$fail" -ne 0 ]; then
        exit 1
      fi

      # Classes as nginx computes them.
      mkdir -p /tmp/nginx
      ${pkgs.nginx}/bin/nginx -c ${classifyConf} -p /tmp/nginx
      for i in $(seq 1 50); do
        ${pkgs.curl}/bin/curl -s -o /dev/null http://127.0.0.1:8080/ && break
        sleep 0.1
      done
      class() {
        got=$(${pkgs.curl}/bin/curl -s -H "X-Real-IP: $2" "http://127.0.0.1:$1/")
        if [ "$got" != "$3" ]; then
          echo "FAIL class of $2 on port $1: expected $3, got $got" >&2
          fail=1
        fi
      }
      class 8080 192.168.1.20 local
      class 8080 10.0.2.15 local
      class 8080 172.18.0.1 local
      class 8080 fd12:3456::1 local
      class 8080 100.101.102.103 tailscale
      class 8080 fd7a:115c:a1e0::5 tailscale
      class 8080 10.213.56.189 netbird
      class 8080 100.70.1.2 netbird
      class 8080 203.0.113.9 web
      class 8080 2a02:1210::1 web
      class 8443 10.213.56.189 web
      class 8443 192.168.1.20 web
      class 8443 203.0.113.9 web
      kill "$(cat /tmp/nginx/nginx.pid)"
      if [ "$fail" -ne 0 ]; then
        exit 1
      fi

      listen=${./listen.nix}
      swag=${../../services/swag/default.nix}
      option=${./reverseProxy.nix}
      patcher=${../../services/swag/swag-patcher.sh}

      if ! grep -q 'ingress-access.conf' "$listen"; then
        echo "FAIL listenHttpsConf must include ingress-access.conf" >&2
        exit 1
      fi
      if ! grep -q 'mkIngressMapsConf' "$swag"; then
        echo "FAIL swag must materialize ingress-maps.conf via mkIngressMapsConf" >&2
        exit 1
      fi
      if ! grep -q 'ingressAccessConf' "$swag"; then
        echo "FAIL swag must materialize ingress-access.conf" >&2
        exit 1
      fi
      if ! grep -q 'ingress-maps.conf' "$patcher"; then
        echo "FAIL swag-patcher must include ingress-maps.conf in nginx.conf" >&2
        exit 1
      fi
      if ! grep -q 'ingress' "$option"; then
        echo "FAIL mkReverseProxyOptions must declare ingress" >&2
        exit 1
      fi

      touch "$out"
    '';
  };
}
