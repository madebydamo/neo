# Paperless reverse proxy for SWAG.
{...}: {
  flake.modules.nixos.paperless-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.paperless;
  in {
    config.neo.services.paperless.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "paperless";
      port = cfg.port;
    });
  };
}
