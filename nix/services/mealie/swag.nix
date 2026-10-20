# Mealie reverse proxy for SWAG.
# UI protected by tinyauth. /api/ bypasses via publicPaths (API tokens, apps, health).
# Recipe zip imports and photos can be large. mkSubdomainProxyConf's default body size is unlimited.
# proxy.conf already sets Upgrade/Connection and proxy timeouts. Do not re-set them.
{...}: {
  flake.modules.nixos.mealie-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.mealie;
  in {
    config.neo.services.mealie.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "mealie";
      port = cfg.port;
    });
  };
}
