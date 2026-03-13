{lib, ...}: {
  libExtensions.authorization = {
    neo = {
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
    };
  };
}
