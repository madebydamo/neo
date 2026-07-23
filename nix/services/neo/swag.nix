# Neo web UI reverse proxy for SWAG (host neo-web on cfg.port).
{...}: {
  flake.modules.nixos.neo-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.neo;
  in {
    config.neo.services.neo.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      proxyPass = "http://host.docker.internal:${toString cfg.port}/";
    });
  };
}
