# Immich-drop reverse proxy for SWAG (no edge tinyauth).
{...}: {
  flake.modules.nixos.immich-drop-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.immich-drop;
  in {
    config.neo.services.immich-drop.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "immich-drop";
      port = 8080;
      auth = false;
    });
  };
}
