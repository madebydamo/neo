# Tinyauth reverse proxy for SWAG (no edge auth on itself).
{...}: {
  flake.modules.nixos.tinyauth-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.tinyauth;
  in {
    config.neo.services.tinyauth.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "tinyauth";
      port = cfg.port;
      maxBodySize = null;
      auth = false;
    });
  };
}
