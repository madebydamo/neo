# Beszel reverse proxy for SWAG (tinyauth + WebSocket).
# proxy.conf already sets Upgrade/Connection — do not re-set them.
{...}: {
  flake.modules.nixos.beszel-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.beszel;
  in {
    config.neo.services.beszel.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "beszel";
      port = 8090;
      maxBodySize = null;
    });
  };
}
