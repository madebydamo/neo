# Example reverse proxy for SWAG: https://<subdomain>.<domain> → http://<name>:<port>.
# mkSubdomainProxyConf adds tinyauth, publicPaths bypasses, ingress and geo rules.
# proxy.conf already sets Upgrade/Connection and proxy timeouts. Do not re-set them.
{...}: {
  neoPlugin.modules.example-swag = {
    config,
    lib,
    plugin,
    ...
  }: let
    name = plugin.name "example";
    cfg = config.neo.services.${name};
  in {
    config.neo.services.${name}.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = name;
      port = cfg.port;
    });
  };
}
