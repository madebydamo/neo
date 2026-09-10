# HTTPS listen + real_ip helpers for SWAG (LAN 443 + PROXY protocol port).
{lib, ...}: {
  libExtensions.listen = {
    neo = rec {
      # Internal container port for PROXY-protocol HTTPS (host maps localHttpsProxyProtocolPort → this).
      httpsProxyProtocolContainerPort = 8443;

      httpsListenInclude = "  include /config/nginx/listen-https.conf;\n";

      # Dual listeners: plain TLS for LAN/Tailscale; PROXY protocol for streamproxy/rathole.
      # ingress-access.conf applies per-vhost posture (local / tailscale / web).
      # error-pages.conf serves the packet-run page for nginx-generated 404s.
      listenHttpsConf = ''
        listen 443 ssl;
        listen [::]:443 ssl;
        listen ${toString httpsProxyProtocolContainerPort} ssl proxy_protocol;
        listen [::]:${toString httpsProxyProtocolContainerPort} ssl proxy_protocol;
        include /config/nginx/ingress-access.conf;
        include /config/nginx/error-pages.conf;
      '';

      # Server-context snippet: nginx-generated 404s (ingress, geo, unknown host,
      # try_files) become the packet-run page. Upstream application 404s pass
      # through unchanged. Asset prefix is reserved so it never collides with
      # an app path and can be exempted from ingress/geo.
      errorPagesConf = ''
        ## Neo-managed — packet-run 404 for nginx-generated 404s.
        error_page 404 =404 /_neo404/index.html;

        location ^~ /_neo404/ {
            alias /config/www/neo-404/p/;
            charset utf-8;
            types {
                text/html html;
                text/css css;
                application/javascript js;
                text/javascript js;
            }
            add_header X-Robots-Tag "noindex, nofollow, nosnippet, noarchive" always;
            add_header Cache-Control "no-store" always;
            error_page 404 =404 /_neo404/asset-missing;
        }

        location = /_neo404/asset-missing {
            internal;
            default_type text/plain;
            return 404 "missing";
        }
      '';

      # Trust private peers that may send PROXY protocol (streamproxy veth, docker, rathole/localhost).
      realIpConf = ''
        set_real_ip_from 10.0.0.0/8;
        set_real_ip_from 172.16.0.0/12;
        set_real_ip_from 192.168.0.0/16;
        set_real_ip_from 127.0.0.1;
        real_ip_header proxy_protocol;
      '';
    };
  };
}
