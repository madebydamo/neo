# Docmost reverse proxy for SWAG.
# UI protected by tinyauth; /api/health bypasses via publicPaths.
# proxy.conf already sets Upgrade/Connection for WebSockets — do not re-set them.
#
# With sso + tinyauth, /login goes to the docmost-sso bridge (after tinyauth),
# which answers with Docmost's authToken cookie and a redirect. Docmost's own
# password, setup and invite flows are closed: accounts and passwords belong
# to the bridge. Auth stays in the access phase: like rustical, try_files
# (content phase) jumps to the named location after tinyauth succeeded.
#
# With mcp, /mcp, the OAuth metadata, register and token endpoints go to
# docmost-mcp without tinyauth (MCP clients cannot follow a login page; /mcp
# checks its own bearer tokens). Only /mcp/oauth/authorize, which the user
# opens in a browser, is behind tinyauth, like /login.
{...}: {
  flake.modules.nixos.docmost-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.docmost;
    tinyauthCfg = config.neo.services.tinyauth;
    ssoActive = cfg.sso && cfg.auth.enabled && tinyauthCfg.enabled;
    mcpActive = ssoActive && cfg.mcp;
    # Must match default.nix.
    swagKey = lib.neo.secrets.sha256 "${cfg.appSecret or ""}:docmost-sso-swag";
    # The key header is derived from appSecret: a rendered snippet (secrets).
    keyHeader = lib.neo.secrets.swagSnippet config "docmost-sso-key" ''
      proxy_set_header X-Neo-SSO-Key "${swagKey}";
    '';
    auth = lib.neo.authBlock config cfg;
    authLoc = lib.neo.authLocations config cfg;
    proxyCore = ''
      include /config/nginx/proxy.conf;
      include /config/nginx/resolver.conf;
      set $upstream_app docmost;
      set $upstream_port ${toString cfg.port};
      set $upstream_proto http;
      proxy_pass $upstream_proto://$upstream_app:$upstream_port;
    '';
    mcpCore = ''
      include /config/nginx/proxy.conf;
      include /config/nginx/resolver.conf;
      set $upstream_app docmost-mcp;
      set $upstream_port 8081;
      proxy_pass http://$upstream_app:$upstream_port;
    '';
    mcpConf = ''
      location = /mcp {
        ${mcpCore}
        # Answers are single JSON bodies, but do not hold back streamed ones.
        proxy_buffering off;
      }

      location ~ ^/(\.well-known/oauth-(protected-resource(/mcp)?|authorization-server)|mcp/oauth/(register|token))$ {
        ${mcpCore}
      }

      location = /mcp/oauth/authorize {
        ${auth}
        try_files /__docmost_mcp_no_such_file @docmost_mcp_authorize;
      }

      location @docmost_mcp_authorize {
        internal;
        # proxy.conf rewrites http:// redirects to https://, which breaks
        # loopback OAuth callbacks (http://localhost:<port>/callback). The first
        # matching mapping wins, so keep those as they are; "off" cannot be
        # combined with proxy.conf's mapping (nginx emerg: duplicate).
        proxy_redirect ~^(http://(localhost|127\.0\.0\.1|\[::1\])([:/?].*)?)$ $1;
        ${mcpCore}
        ${keyHeader.include}
        proxy_set_header Remote-User $user;
        proxy_set_header Remote-Email $email;
        proxy_set_header Remote-Name $name;
        proxy_set_header Remote-Groups $groups;
      }
    '';
    ssoConf = ''
      server {
        include /config/nginx/listen-https.conf;
        http2 on;
        server_name ${cfg.subdomain}.*;
        include /config/nginx/ssl.conf;
        # Wiki attachments / page assets can be large.
        client_max_body_size 100M;
        include /config/nginx/geo-access.conf;
        # Streamproxy/rathole terminates on SWAG :8443 (PROXY protocol).
        absolute_redirect off;
        port_in_redirect off;

        location = /login {
          ${auth}
          try_files /__docmost_sso_no_such_file @docmost_sso;
        }

        location @docmost_sso {
          internal;
          include /config/nginx/proxy.conf;
          include /config/nginx/resolver.conf;
          set $upstream_app docmost-sso;
          set $upstream_port 8080;
          ${keyHeader.include}
          proxy_set_header Remote-User $user;
          proxy_set_header Remote-Email $email;
          proxy_set_header Remote-Name $name;
          proxy_set_header Remote-Groups $groups;
          proxy_pass http://$upstream_app:$upstream_port;
        }

        # Password sign-in, setup and invite acceptance: only the bridge
        # (straight to docmost:3000 on the internal network) may use these.
        location ~ ^/api/(auth/(login|setup|change-password|forgot-password|password-reset)|workspace/invites/accept)$ {
          return 403;
        }

        # Their pages: back to tinyauth sign-in.
        location ~ ^/(setup|signup|forgot-password|password-reset|invites)(/|$) {
          return 302 /login;
        }

        ${lib.optionalString mcpActive mcpConf}
        location / {
      ${proxyCore} ${auth}
        }

        ${authLoc}
      }
    '';
  in {
    config.neo.core.secrets.templates = lib.mkIf (cfg.enabled && ssoActive) keyHeader.templates;

    config.neo.services.docmost.proxyConf = lib.mkDefault (
      if ssoActive
      then ssoConf
      else
        lib.neo.mkSubdomainProxyConf {
          inherit config cfg;
          upstream = "docmost";
          port = cfg.port;
          # Wiki attachments / page assets can be large.
          maxBodySize = "100M";
        }
    );
  };
}
