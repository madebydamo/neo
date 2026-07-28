# Tinyauth snippets and standard SWAG subdomain server block helper.
{lib, ...}: {
  libExtensions.authorization = {
    neo = rec {
      authBlock = config: cfg: let
        tinyauthCfg = config.neo.services.tinyauth;
        authEnabled = cfg.auth.enabled && tinyauthCfg.enabled;
      in
        lib.optionalString authEnabled ''

          # Tinyauth forward authentication
          auth_request /tinyauth;
          error_page 401 = @tinyauth_login;
        '';
      authLocations = config: cfg: let
        domain = config.neo.services.swag.domain;
        tinyauthCfg = config.neo.services.tinyauth;
        authEnabled = cfg.auth.enabled && tinyauthCfg.enabled;
      in
        lib.optionalString authEnabled ''

          # Tinyauth auth request handler
          location /tinyauth {
            internal;
            proxy_pass http://tinyauth:${toString tinyauthCfg.port}/api/auth/nginx;
            proxy_set_header x-forwarded-proto $scheme;
            proxy_set_header x-forwarded-host $http_host;
            proxy_set_header x-forwarded-uri $request_uri;
          }

          # Tinyauth login redirect
          location @tinyauth_login {
            return 302 https://${tinyauthCfg.subdomain}.${domain}/login?redirect_uri=$scheme://$http_host$request_uri;
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
