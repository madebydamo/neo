# HTTPS listen + real_ip helpers for SWAG (LAN 443 + PROXY protocol port).
{lib, ...}: {
  libExtensions.listen = {
    neo = rec {
      # Internal container port for PROXY-protocol HTTPS (host maps localHttpsProxyProtocolPort → this).
      httpsProxyProtocolContainerPort = 8443;

      httpsListenInclude = "  include /config/nginx/listen-https.conf;\n";

      # Dual listeners: plain TLS for LAN; PROXY protocol for streamproxy/rathole.
      listenHttpsConf = ''
        listen 443 ssl;
        listen [::]:443 ssl;
        listen ${toString httpsProxyProtocolContainerPort} ssl proxy_protocol;
        listen [::]:${toString httpsProxyProtocolContainerPort} ssl proxy_protocol;
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
