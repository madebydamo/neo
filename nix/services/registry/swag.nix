# Docker registry reverse proxy for SWAG.
# No tinyauth: clients authenticate against the registry's own htpasswd.
# Layers can be GBs: no body limit, stream uploads and downloads unbuffered.
# proxy.conf already sets Upgrade/Connection and proxy timeouts — do not re-set them.
{...}: {
  flake.modules.nixos.registry-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.registry;
  in {
    config.neo.services.registry.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      upstream = "registry";
      port = cfg.port;
      auth = false;
      serverExtra = "  chunked_transfer_encoding on;\n";
      locationExtra = "    proxy_request_buffering off;\n    proxy_buffering off;";
    });
  };
}
