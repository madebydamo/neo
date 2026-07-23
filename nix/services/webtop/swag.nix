# Webtop reverse proxy for SWAG (HTTP on CUSTOM_PORT / cfg.port).
# proxy.conf already sets Upgrade/Connection — do not re-set them.
{...}: {
  flake.modules.nixos.webtop-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.webtop;
  in {
    config.neo.services.webtop.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "webtop";
      port = cfg.port;
    });
  };
}
