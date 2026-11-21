# ConvertX reverse proxy for SWAG.
# Uploads can be large; mkSubdomainProxyConf's default body size is unlimited.
# proxy.conf already sets Upgrade/Connection and proxy timeouts. Do not re-set them.
{...}: {
  flake.modules.nixos.convertx-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.convertx;
  in {
    config.neo.services.convertx.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "convertx";
      port = cfg.port;
    });
  };
}
