# Firefox reverse proxy for SWAG (HTTP on CUSTOM_PORT / cfg.port).
# proxy.conf already sets Upgrade/Connection — do not re-set them.
{...}: {
  flake.modules.nixos.firefox-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.firefox;
  in {
    config.neo.services.firefox.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "firefox";
      port = cfg.port;
    });
  };
}
