# Immich reverse proxy for SWAG.
{...}: {
  flake.modules.nixos.immich-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.immich;
  in {
    config.neo.services.immich.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "immich-server";
      port = 2283;
    });
  };
}
