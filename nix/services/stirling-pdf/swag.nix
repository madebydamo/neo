# Stirling PDF reverse proxy for SWAG.
# Large uploads via client_max_body_size 0. Do not re-set proxy_*_timeout or
# Upgrade/Connection — already in proxy.conf (duplicates break nginx).
{...}: {
  flake.modules.nixos.stirling-pdf-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.stirling-pdf;
  in {
    config.neo.services.stirling-pdf.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "stirling-pdf";
      port = cfg.port;
    });
  };
}
