# Karakeep reverse proxy for SWAG.
# UI behind tinyauth; /api on publicPaths for extensions/apps.
{...}: {
  flake.modules.nixos.karakeep-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.karakeep;
  in {
    config.neo.services.karakeep.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "karakeep";
      port = cfg.port;
      maxBodySize = "100M";
    });
  };
}
