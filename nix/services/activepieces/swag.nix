# Activepieces reverse proxy for SWAG.
# UI protected by tinyauth; webhook paths bypass via publicPaths.
# proxy.conf already sets Upgrade/Connection — do not re-set them.
{...}: {
  flake.modules.nixos.activepieces-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.activepieces;
  in {
    config.neo.services.activepieces.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "activepieces";
      port = cfg.port;
    });
  };
}
