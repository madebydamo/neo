# Syncthing reverse proxy configuration for SWAG.
{...}: {
  flake.modules.nixos.syncthing-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.syncthing;
  in {
    config.neo.services.syncthing.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "syncthing";
      port = 8384;
      locationExtra = "    proxy_hide_header Authorization;";
      # REST API: no tinyauth, clients use their API key.
      extraLocations = ''

        location ~ (/syncthing)?/rest {
          ${lib.neo.swagUpstream {
          app = "syncthing";
          port = 8384;
        }}
          proxy_hide_header Authorization;
        }
      '';
    });
  };
}
