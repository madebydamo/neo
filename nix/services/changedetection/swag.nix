# Changedetection reverse proxy for SWAG.
{...}: {
  flake.modules.nixos.changedetection-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.changedetection;
  in {
    config.neo.services.changedetection.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "changedetection";
      port = cfg.port;
    });
  };
}
