# Searxng reverse proxy for SWAG.
{...}: {
  flake.modules.nixos.searxng-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.searxng;
  in {
    config.neo.services.searxng.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "searxng";
      port = 8080;
      maxBodySize = null;
    });
  };
}
