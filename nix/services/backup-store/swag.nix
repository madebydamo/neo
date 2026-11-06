# Backup store reverse proxy for SWAG: the WebSocket tunnel on the host
# (host.docker.internal reaches it via mkDockerToLocalhostForward).
# proxy.conf already sets Upgrade/Connection and timeouts — do not re-set them.
# No tinyauth: SSH clients log in with their key at the end of the tunnel.
{...}: {
  flake.modules.nixos.backup-store-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.backup-store;
  in {
    config.neo.services.backup-store.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      proxyPass = "http://host.docker.internal:${toString cfg.tunnelPort}/";
    });
  };
}
