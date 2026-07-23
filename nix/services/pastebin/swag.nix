# Pastebin reverse proxy for SWAG.
# Tinyauth is disabled by default (see option.nix).
{...}: {
  flake.modules.nixos.pastebin-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.pastebin;
  in {
    config.neo.services.pastebin.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "pastebin";
      port = cfg.port;
    });
  };
}
