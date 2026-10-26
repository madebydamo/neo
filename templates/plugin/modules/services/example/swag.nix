# Example reverse proxy for SWAG: https://<subdomain>.<domain> → http://example:<port>.
# mkSubdomainProxyConf adds tinyauth, publicPaths bypasses, ingress and geo rules.
# proxy.conf already sets Upgrade/Connection and proxy timeouts. Do not re-set them.
{...}: {
  flake.modules.nixos.example-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.example;
  in {
    config.neo.services.example.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "example";
      port = cfg.port;
    });
  };
}
