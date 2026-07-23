# Filebrowser reverse proxy for SWAG.
{...}: {
  flake.modules.nixos.filebrowser-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.filebrowser;
  in {
    config.neo.services.filebrowser.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "filebrowser";
      port = 8080;
    });
  };
}
