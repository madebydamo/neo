# Dufs reverse proxy for SWAG.
# UI protected by tinyauth; /__dufs__/health bypasses via publicPaths.
# When password is set, DAV methods and Authorization-bearing requests skip
# tinyauth (native WebDAV clients use HTTP Basic, not cookies). GET / stays
# authenticated; SWAG injects dufs Basic credentials after tinyauth so the
# browser UI does not show a second login.
#
# Auth must stay in the access phase: `return`/`if` in the same location as
# tinyauth-location.conf runs in rewrite and skips auth_request. Exception:
# DAV methods / Authorization return 418 in rewrite on purpose so they never
# hit tinyauth or the SSO 302.
#
# proxy.conf already sets Upgrade/Connection and proxy timeouts — do not re-set them.
{...}: {
  flake.modules.nixos.dufs-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.dufs;
    webdavAuth =
      (cfg.password or null)
      != null
      && cfg.password != "";
    # RFC 4648 base64; this nixpkgs lib.strings has no toBase64.
    toBase64 = str: let
      inherit (lib) stringToCharacters concatStrings elemAt;
      inherit (lib.strings) charToInt;
      alphabet = stringToCharacters "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
      bytes = map charToInt (stringToCharacters str);
      len = builtins.length bytes;
      at = i:
        if i < len
        then elemAt bytes i
        else 0;
      nChunks = (len + 2) / 3;
      chunk = i: let
        n = (at (i * 3)) * 65536 + (at (i * 3 + 1)) * 256 + (at (i * 3 + 2));
        c0 = elemAt alphabet (n / 262144);
        c1 = elemAt alphabet (lib.mod (n / 4096) 64);
        c2 = elemAt alphabet (lib.mod (n / 64) 64);
        c3 = elemAt alphabet (lib.mod n 64);
        remaining = len - i * 3;
      in
        if remaining == 1
        then c0 + c1 + "=="
        else if remaining == 2
        then c0 + c1 + c2 + "="
        else c0 + c1 + c2 + c3;
    in
      concatStrings (map chunk (lib.range 0 (nChunks - 1)));
    auth = lib.neo.authBlock config cfg;
    authLoc = lib.neo.authLocations config cfg;
    proxyCore = ''
      include /config/nginx/proxy.conf;
      include /config/nginx/resolver.conf;
      set $upstream_app dufs;
      set $upstream_port ${toString cfg.port};
      set $upstream_proto http;
      proxy_set_header X-Forwarded-Port 443;
      proxy_request_buffering off;
      proxy_pass $upstream_proto://$upstream_app:$upstream_port;
    '';
    injectAuth = lib.optionalString webdavAuth ''
      proxy_set_header Authorization "Basic ${toBase64 "${cfg.username}:${cfg.password}"}";
    '';
    davSkipAuth = lib.optionalString webdavAuth ''
      error_page 418 = @dufs_dav;
      if ($request_method ~ ^(OPTIONS|PROPFIND|PROPPATCH|MKCOL|COPY|MOVE|LOCK|UNLOCK)$) {
        return 418;
      }
      if ($http_authorization != "") {
        return 418;
      }
    '';
    davNamed = lib.optionalString webdavAuth ''
      location @dufs_dav {
        internal;
      ${proxyCore}
      }
    '';
  in {
    config.neo.services.dufs.proxyConf = lib.mkDefault ''
      server {
        include /config/nginx/listen-https.conf;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;
        client_max_body_size 0;
        include /config/nginx/geo-access.conf;

        location = /__dufs__/health {
          ${proxyCore}
        }

        location / {
          ${davSkipAuth}
          ${proxyCore}
          ${injectAuth}
          ${auth}
        }

        ${davNamed}

        ${authLoc}
      }
    '';
  };
}
