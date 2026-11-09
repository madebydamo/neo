# Pi-hole reverse proxy configuration for SWAG.
{...}: {
  flake.modules.nixos.pihole-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.pihole;
  in {
    config.neo.services.pihole.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "pihole";
      port = 80;
    });
  };
}
