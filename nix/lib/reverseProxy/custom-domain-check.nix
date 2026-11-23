# Custom domains of an app with login redirect (307) to the app's own address
# before any auth, also below the SWAG domain (biel.dev.example.com under
# example.com: meilen 2026-11-23, tinyauth 400 → nginx 500); without login they
# proxy. The tinyauth subrequest drops forward-auth headers, which tinyauth
# refuses next to X-Original-URL.
{...}: {
  perSystem = {
    pkgs,
    lib,
    ...
  }: let
    rp = (import ./reverseProxy.nix {inherit lib;}).libExtensions.reverseProxy.neo;
    authz = (import ./authorization.nix {inherit lib;}).libExtensions.authorization.neo;
    tinyauthServerConf = authz.mkTinyauthServerConf {neo.services.tinyauth.port = 3000;};

    domain = "dev1.example.com";
    redirects = customDomain: authEnabled: rp.customDomainRedirects {inherit domain customDomain authEnabled;};
    conf = customDomain: redirect:
      rp.mkCustomDomainSiteConf {
        inherit domain customDomain redirect;
        subdomain = "kpi";
      };
    redirectConf = conf "kpi.dev.example.com" true;
    proxyConf = conf "www.example.org" false;

    expectTrue = name: cond:
      if cond
      then ""
      else ''
        echo "FAIL ${name}" >&2
        fail=1
      '';

    body = lib.concatStrings [
      (expectTrue "login-foreign-domain-redirects" (redirects "kpi.dev.example.com" true))
      (expectTrue "login-other-domain-redirects" (redirects "www.example.org" true))
      (expectTrue "no-login-proxies" (!redirects "www.example.org" false))
      (expectTrue "below-swag-domain-redirects" (redirects "kpi-old.dev1.example.com" true))
      (expectTrue "two-below-swag-domain-redirects" (redirects "kpi.old.dev1.example.com" true))
      (expectTrue "swag-domain-itself-redirects" (redirects "dev1.example.com" true))
      (expectTrue "suffix-without-dot-redirects" (redirects "xdev1.example.com" true))
      (expectTrue "below-swag-domain-no-login-proxies" (!redirects "kpi-old.dev1.example.com" false))
      (expectTrue "auth-subrequest-drops-forwarded-host" (lib.hasInfix ''proxy_set_header X-Forwarded-Host "";'' tinyauthServerConf))
      (expectTrue "auth-subrequest-drops-forwarded-uri" (lib.hasInfix ''proxy_set_header X-Forwarded-Uri "";'' tinyauthServerConf))
      (expectTrue "redirect-is-307-to-own-address" (lib.hasInfix "return 307 https://kpi.dev1.example.com$request_uri;" redirectConf))
      (expectTrue "redirect-no-auth" (!lib.hasInfix "tinyauth" redirectConf && !lib.hasInfix "proxy_pass" redirectConf))
      (expectTrue "redirect-keeps-geo" (lib.hasInfix "geo-access.conf" redirectConf))
      (expectTrue "proxy-unchanged" (lib.hasInfix "proxy_set_header Host kpi.dev1.example.com;" proxyConf && !lib.hasInfix "return 307" proxyConf))
    ];

    # The rendered confs without SWAG's includes, on unprivileged ports.
    serve = c:
      lib.replaceStrings
      [
        "listen 80;"
        "listen [::]:80;"
        "include /config/nginx/listen-https.conf;"
        "http2 on;"
        "include /config/nginx/ssl.conf;"
        "include /config/nginx/geo-access.conf;"
      ]
      ["listen 8080;" "" "listen 8081;" "" "" ""]
      c;
    nginxConf = pkgs.writeText "custom-domain-nginx.conf" ''
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
        ${serve redirectConf}
      }
    '';
  in {
    checks.custom-domain-login = pkgs.runCommand "custom-domain-login" {} ''
      set -euo pipefail
      fail=0
      ${body}
      if [ "$fail" -ne 0 ]; then
        exit 1
      fi

      mkdir -p /tmp/nginx
      ${pkgs.nginx}/bin/nginx -c ${nginxConf} -p /tmp/nginx
      for i in $(seq 1 50); do
        ${pkgs.curl}/bin/curl -s -o /dev/null http://127.0.0.1:8081/ && break
        sleep 0.1
      done
      got=$(${pkgs.curl}/bin/curl -s -o /dev/null -w '%{http_code} %{redirect_url}' -X POST -H 'Host: kpi.dev.example.com' 'http://127.0.0.1:8081/a/b?c=d')
      if [ "$got" != "307 https://kpi.dev1.example.com/a/b?c=d" ]; then
        echo "FAIL custom domain with login: got '$got'" >&2
        fail=1
      fi
      got=$(${pkgs.curl}/bin/curl -s -o /dev/null -w '%{http_code}' -H 'Host: kpi.dev.example.com' 'http://127.0.0.1:8080/x')
      if [ "$got" != "301" ]; then
        echo "FAIL port 80 of custom domain: got '$got'" >&2
        fail=1
      fi
      kill "$(cat /tmp/nginx/nginx.pid)"
      if [ "$fail" -ne 0 ]; then
        exit 1
      fi
      touch "$out"
    '';
  };
}
