# Tinyauth snippets and standard SWAG subdomain server block helper.
#
# Uses SWAG's include-based integration so swag-dashboard can detect auth:
#   TINYAUTH_REGEX = r"\n\s+include \/config\/nginx\/tinyauth-location\.conf;.*"
# authBlock  → location-level include (tinyauth-location.conf)
# authLocations → server-level include (tinyauth-server.conf)
{lib, ...}: let
  inherit ((import ./geo.nix {inherit lib;}).libExtensions.geo.neo) geoAccessInclude;
  inherit ((import ./listen.nix {inherit lib;}).libExtensions.listen.neo) httpsListenInclude;
in {
  libExtensions.authorization = {
    neo = rec {
      # Location-block snippet. Leading "\n  " is required for the dashboard regex.
      authBlock = config: cfg: let
        tinyauthCfg = config.neo.services.tinyauth;
        authEnabled = cfg.auth.enabled && tinyauthCfg.enabled;
      in
        lib.optionalString authEnabled "\n    include /config/nginx/tinyauth-location.conf;";

      # Server-block snippet: internal /tinyauth auth subrequest.
      authLocations = config: cfg: let
        tinyauthCfg = config.neo.services.tinyauth;
        authEnabled = cfg.auth.enabled && tinyauthCfg.enabled;
      in
        lib.optionalString authEnabled "\n  include /config/nginx/tinyauth-server.conf;";

      # Materialized as /config/nginx/tinyauth-location.conf.
      # Redirect target comes from Tinyauth (X-Tinyauth-Location), including 403.
      tinyauthLocationConf = ''
        ## Send a subrequest to tinyauth to verify if the user is authenticated and has permission to access the resource
        auth_request /tinyauth;
        auth_request_set $redirection_url $upstream_http_x_tinyauth_location;
        error_page 401 403 =302 $redirection_url;

        ## Translate the user information response headers from the auth subrequest into variables
        auth_request_set $email $upstream_http_remote_email;
        auth_request_set $groups $upstream_http_remote_groups;
        auth_request_set $name $upstream_http_remote_name;
        auth_request_set $user $upstream_http_remote_user;

        ## Inject the user information into the request made to the actual upstream
        proxy_set_header Remote-Email $email;
        proxy_set_header Remote-Groups $groups;
        proxy_set_header Remote-Name $name;
        proxy_set_header Remote-User $user;
      '';

      # Materialized as /config/nginx/tinyauth-server.conf.
      # Port stays neo's; the sample hardcodes 3000. No proxy.conf here:
      # that file's Host/Upgrade headers do not belong on the auth subrequest.
      mkTinyauthServerConf = config: let
        tinyauthCfg = config.neo.services.tinyauth;
        port = toString tinyauthCfg.port;
      in ''
        # location for tinyauth auth requests
        location /tinyauth {
            internal;

            include /config/nginx/resolver.conf;
            set $upstream_tinyauth tinyauth;
            proxy_pass http://$upstream_tinyauth:${port}/api/auth/nginx;

            # Don't send the body to the auth server
            proxy_pass_request_body off;
            proxy_set_header Content-Length "";

            # Headers needed for authentication
            proxy_set_header X-Original-URL $scheme://$http_host$request_uri;
            proxy_set_header X-Original-Method $request_method;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Real-IP $remote_addr;
        }
      '';

      # Lines of a location body that proxies to a container on the internal
      # network. Variables make nginx resolve the name at request time, so SWAG
      # starts while the container is down.
      # `extra` goes before proxy_pass. proxyConf = false skips proxy.conf, for a
      # location that sets its own headers (nginx appends, it does not override).
      # webSocket = true (implies proxyConf = false): the WebSocket upgrade and
      # forwarded headers proxy.conf would set, written out once, so `timeout`
      # can set proxy_read/send_timeout (proxy.conf's 240 s cannot be set twice).
      swagUpstream = {
        app,
        port,
        proto ? "http",
        webSocket ? false,
        proxyConf ? !webSocket,
        timeout ? null,
        extra ? "",
      }:
        assert lib.assertMsg (!(webSocket && proxyConf)) "swagUpstream ${app}: webSocket = true needs proxyConf = false (proxy.conf already sets Upgrade/Connection)";
        assert lib.assertMsg (timeout == null || !proxyConf) "swagUpstream ${app}: timeout needs proxyConf = false (proxy.conf already sets the proxy timeouts)";
          lib.optionalString proxyConf "include /config/nginx/proxy.conf;\n"
          + "include /config/nginx/resolver.conf;\n"
          + "set $upstream_app ${app};\n"
          + "set $upstream_port ${toString port};\n"
          + "set $upstream_proto ${proto};\n"
          + lib.optionalString webSocket webSocketHeaders
          + lib.optionalString (timeout != null) "proxy_read_timeout ${timeout};\nproxy_send_timeout ${timeout};\n"
          + extra
          + "proxy_pass $upstream_proto://$upstream_app:$upstream_port;\n";

      # What proxy.conf sets for a WebSocket, for locations without it (swagUpstream webSocket).
      webSocketHeaders = ''
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
      '';

      # The server block every vhost shares: TLS, HTTP/2, geo rules, body limit.
      # `extra` holds more server-level directives and `body` the locations.
      mkSwagServer = {
        cfg,
        maxBodySize ? "0",
        geo ? true,
        extra ? "",
        body,
      }:
        "server {\n"
        + httpsListenInclude
        + "  http2 on;\n"
        + "  server_name ${cfg.subdomain}.*;\n"
        + "  include /config/nginx/ssl.conf;\n"
        + lib.optionalString (maxBodySize != null) "\n  client_max_body_size ${maxBodySize};\n"
        + lib.optionalString geo geoAccessInclude
        + extra
        + "\n"
        + body
        + "\n"
        + "}\n";

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
        # Extra nginx server-level directives (e.g. add_header), before `location /`.
        serverExtra ? "",
        # Extra directives in `location /`, after proxy_pass.
        locationExtra ? "",
        # Extra nginx server-block text (e.g. location blocks), after `location /`.
        extraLocations ? "",
      }: let
        ab =
          if auth
          then authBlock config cfg
          else "";
        al =
          if auth
          then authLocations config cfg
          else "";
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
        mkSwagServer {
          inherit cfg maxBodySize geo;
          extra = serverExtra;
          body =
            "  location / {\n"
            + "    include /config/nginx/proxy.conf;\n"
            + resolverLine
            + pass
            + lib.optionalString (locationExtra != "") "\n${locationExtra}"
            + ab
            + "\n"
            + "  }\n"
            + al
            + extraLocations;
        };
    };
  };
}
