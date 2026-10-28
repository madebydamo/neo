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
    # The UI Basic header carries the password: a rendered snippet (secrets).
    authSnippet = lib.neo.secrets.swagSnippet config "dufs-auth" ''
      proxy_set_header Authorization "Basic ${lib.neo.secrets.base64 "${cfg.username}:${cfg.password}"}";
    '';
    injectAuth = lib.optionalString webdavAuth authSnippet.include;
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
    config.neo.core.secrets.templates = lib.mkIf (cfg.enabled && webdavAuth) authSnippet.templates;

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
