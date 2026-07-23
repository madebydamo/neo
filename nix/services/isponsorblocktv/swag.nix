# iSponsorBlockTV reverse proxy for SWAG (ttyd setup UI).
# proxy.conf already sets Upgrade/Connection — do not re-set them.
{...}: {
  flake.modules.nixos.isponsorblocktv-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.isponsorblocktv;
  in {
    config.neo.services.isponsorblocktv.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      proxyPass = "http://host.docker.internal:7681";
      maxBodySize = null;
    });
  };
}
