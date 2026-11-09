# Vaultwarden reverse proxy configuration for SWAG.
# Adds tinyauth forward auth for the web UI (/ and /admin); API and notifications paths are public
# (bypassed via publicPaths and no authBlock) to support Bitwarden clients and websocket.
{...}: {
  flake.modules.nixos.vaultwarden-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.vaultwarden;
    upstream = lib.neo.swagUpstream {
      app = "vaultwarden";
      port = cfg.port;
    };
  in {
    config.neo.services.vaultwarden.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "vaultwarden";
      port = cfg.port;
      maxBodySize = "128M";
      extraLocations = ''

        location ~ ^(/vaultwarden)?/admin {
          ${upstream}
          ${lib.neo.authBlock config cfg}
        }

        location ~ (/vaultwarden)?/api {
          ${upstream}
        }

        location ~ (/vaultwarden)?/notifications/hub {
          ${upstream}
        }
      '';
    });
  };
}
