# Calino reverse proxy for SWAG.
# UI protected by tinyauth. Upstream Caddy sets X-Frame-Options SAMEORIGIN;
# SWAG proxy.conf hides that header when neo.iframeCookieSupport is on.
{...}: {
  flake.modules.nixos.calino-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.calino;
  in {
    config.neo.services.calino.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "calino";
      port = cfg.port;
    });
  };
}
