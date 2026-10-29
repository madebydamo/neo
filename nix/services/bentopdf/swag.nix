# BentoPDF reverse proxy for SWAG. Processing is client-side, so no large
# upload limit is needed; the container's own COOP/COEP headers pass through.
{...}: {
  flake.modules.nixos.bentopdf-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.bentopdf;
  in {
    config.neo.services.bentopdf.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "bentopdf";
      port = cfg.port;
    });
  };
}
