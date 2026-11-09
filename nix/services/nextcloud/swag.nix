# Nextcloud reverse proxy configuration for SWAG.
# Protects the Nextcloud web UI with tinyauth forward authentication (additional auth for webui).
{...}: {
  flake.modules.nixos.nextcloud-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.nextcloud;
  in {
    config.neo.services.nextcloud.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "nextcloud";
      port = cfg.port;
      serverExtra = ''
        add_header X-Content-Type-Options "nosniff" always;
        add_header X-Robots-Tag "noindex,nofollow" always;
        add_header Referrer-Policy "strict-origin-when-cross-origin" always;
        add_header Strict-Transport-Security "max-age=15552000; includeSubDomains" always;
      '';
      locationExtra = ''
        proxy_hide_header Referrer-Policy;
        proxy_hide_header X-Content-Type-Options;
        proxy_hide_header X-Frame-Options;
        proxy_hide_header X-XSS-Protection;
        proxy_buffering off;
      '';
      extraLocations = ''

        # Support for JavaScript modules (.mjs MIME type)
        location ~* \.(?:mjs|js|css|woff2?|ttf|svg|png|jpg|jpeg|gif|ico)$ {
          ${lib.neo.swagUpstream {
          app = "nextcloud";
          port = cfg.port;
        }}
          # Force correct MIME type for ES modules
          default_type application/javascript;

          expires 6M;
          access_log off;
        }
      '';
    });
  };
}
