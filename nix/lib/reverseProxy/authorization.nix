# Tinyauth snippets and standard SWAG subdomain server block helper.
#
# Uses SWAG's include-based integration so swag-dashboard can detect auth:
#   TINYAUTH_REGEX = r"\n\s+include \/config\/nginx\/tinyauth-location\.conf;.*"
# authBlock  → location-level include (tinyauth-location.conf)
# authLocations → server-level include (tinyauth-server.conf)
{lib, ...}: {
  libExtensions.authorization = {
    neo = rec {
      # Location-block snippet. Leading "\n  " is required for the dashboard regex.
      authBlock = config: cfg: let
        tinyauthCfg = config.neo.services.tinyauth;
        authEnabled = cfg.auth.enabled && tinyauthCfg.enabled;
      in
        lib.optionalString authEnabled "\n    include /config/nginx/tinyauth-location.conf;";

      # Server-block snippet: /tinyauth auth handler + @tinyauth_login redirect.
      authLocations = config: cfg: let
        tinyauthCfg = config.neo.services.tinyauth;
        authEnabled = cfg.auth.enabled && tinyauthCfg.enabled;
      in
        lib.optionalString authEnabled "\n  include /config/nginx/tinyauth-server.conf;";

      # Materialized as /config/nginx/tinyauth-location.conf (SWAG-compatible).
      tinyauthLocationConf = ''
        ## Neo-managed — SWAG tinyauth location snippet
        ## Include inside location / { ... } when edge auth is enabled.

        ## Send a subrequest to tinyauth to verify if the user is authenticated
        auth_request /tinyauth;

        ## If the subrequest returns 200 pass to the backend; 401 → login portal
        error_page 401 = @tinyauth_login;

        ## Translate user info response headers from the auth subrequest
        auth_request_set $email $upstream_http_remote_email;
        auth_request_set $groups $upstream_http_remote_groups;
        auth_request_set $name $upstream_http_remote_name;
        auth_request_set $user $upstream_http_remote_user;

        ## Inject user information into the upstream request
        proxy_set_header Remote-Email $email;
        proxy_set_header Remote-Groups $groups;
        proxy_set_header Remote-Name $name;
        proxy_set_header Remote-User $user;
      '';

      # Materialized as /config/nginx/tinyauth-server.conf (port/subdomain from neo).
      mkTinyauthServerConf = config: let
        tinyauthCfg = config.neo.services.tinyauth;
        domain = config.neo.services.swag.domain;
        port = toString tinyauthCfg.port;
        subdomain = tinyauthCfg.subdomain;
      in ''
        ## Neo-managed — SWAG tinyauth server snippet
        ## Include in the server { ... } block when edge auth is enabled.

        # location for tinyauth auth requests
        location /tinyauth {
            internal;

            include /config/nginx/proxy.conf;
            include /config/nginx/resolver.conf;
            set $upstream_tinyauth tinyauth;
            proxy_pass http://$upstream_tinyauth:${port}/api/auth/nginx;

            proxy_set_header x-forwarded-proto $scheme;
            proxy_set_header x-forwarded-host $http_host;
            proxy_set_header x-forwarded-uri $request_uri;
        }

        # virtual location for tinyauth 401 redirects
        location @tinyauth_login {
            internal;
            return 302 https://${subdomain}.${domain}/login?redirect_uri=$scheme://$http_host$request_uri;
        }
      '';

      # Standard SWAG subdomain vhost: TLS + proxy.conf + optional tinyauth + geo.
      mkSubdomainProxyConf = {
        config,
        cfg,
        upstream ? null,
        port ? null,
        proto ? "http",
        proxyPass ? null,
        maxBodySize ? "0",
        auth ? true,
        includeResolver ? (proxyPass == null),
        geo ? true,
      }: let
        ab =
          if auth
          then authBlock config cfg
          else "";
        al =
          if auth
          then authLocations config cfg
          else "";
        geoLine =
          if geo
          then "  include /config/nginx/geo-access.conf;\n"
          else "";
        bodySize =
          if maxBodySize == null
          then ""
          else "\n  client_max_body_size ${maxBodySize};\n";
        resolverLine = lib.optionalString includeResolver "    include /config/nginx/resolver.conf;\n";
        pass =
          if proxyPass != null
          then "    proxy_pass ${proxyPass};"
          else
            "    set $upstream_app ${upstream};\n"
            + "    set $upstream_port ${toString port};\n"
            + "    set $upstream_proto ${proto};\n"
            + "    proxy_pass $upstream_proto://$upstream_app:$upstream_port;";
      in
        "server {\n"
        + "  include /config/nginx/listen-https.conf;\n"
        + "  http2 on;\n"
        + "  server_name ${cfg.subdomain}.*;\n"
        + "  include /config/nginx/ssl.conf;\n"
        + bodySize
        + geoLine
        + "\n"
        + "  location / {\n"
        + "    include /config/nginx/proxy.conf;\n"
        + resolverLine
        + pass
        + ab
        + "\n"
        + "  }\n"
        + al
        + "\n"
        + "}\n";
    };
  };
}
