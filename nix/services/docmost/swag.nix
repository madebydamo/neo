# Docmost reverse proxy for SWAG.
# UI protected by tinyauth; /api/health bypasses via publicPaths.
# proxy.conf already sets Upgrade/Connection for WebSockets — do not re-set them.
{...}: {
  flake.modules.nixos.docmost-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.docmost;
  in {
    config.neo.services.docmost.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "docmost";
      port = cfg.port;
      # Wiki attachments / page assets can be large.
      maxBodySize = "100M";
    });
  };
}
