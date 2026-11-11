# NetBird control plane behind SWAG. One host name carries four kinds of
# traffic: the relay and management WebSockets, gRPC (signal, management),
# the HTTP API with the login pages, and the dashboard.
{...}: {
  flake.modules.nixos.netbird-server-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.netbird-server;
    server = "netbird-server";
    # proxy.conf limits a silent connection to 240 s and its timeouts cannot be
    # set twice, so the long-lived WebSockets carry their own headers.
    webSocket = lib.neo.swagUpstream {
      app = server;
      port = 80;
      proxyConf = false;
      extra = ''
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 1d;
        proxy_send_timeout 1d;
      '';
    };
  in {
    config.neo.services.netbird-server.proxyConf = lib.mkDefault (lib.neo.mkSwagServer {
      inherit cfg;
      # The country filter applies to people (dashboard, login, API), not to
      # devices: a phone abroad would lose the whole network. Device traffic
      # authenticates with keys.
      geo = false;
      extra = ''
        # gRPC streams stay open for as long as a device is connected.
        client_header_timeout 1d;
        client_body_timeout 1d;
      '';
      body = ''
        location ~ ^/(relay|ws-proxy/) {
        ${webSocket}
        }

        location ~ ^/(signalexchange\.SignalExchange|management\.ManagementService|management\.ProxyService)/ {
          include /config/nginx/resolver.conf;
          set $upstream_grpc ${server};
          grpc_set_header Host $host;
          grpc_set_header X-Real-IP $remote_addr;
          grpc_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
          grpc_read_timeout 1d;
          grpc_send_timeout 1d;
          grpc_socket_keepalive on;
          grpc_pass grpc://$upstream_grpc:80;
        }

        # The first-run page creates the owner without a login: only the setup
        # unit may call it, over the internal network.
        location = /api/setup {
          return 404;
        }

        location ~ ^/(api|oauth2)/ {
          include /config/nginx/geo-access.conf;
        ${lib.neo.swagUpstream {
          app = server;
          port = 80;
        }}
        }

        location / {
          include /config/nginx/geo-access.conf;
        ${lib.neo.swagUpstream {
          app = "netbird-dashboard";
          port = 80;
        }}
        }
      '';
    });
  };
}
