# Vikunja reverse proxy for SWAG (host vikunja on cfg.port).
# Binds 127.0.0.1; host.docker.internal reaches it via mkDockerToLocalhostForward.
# host.docker.internal is already on the SWAG container — do not add it again.
# proxy.conf already sets Upgrade/Connection and X-Forwarded-* — do not re-set them.
# API, CalDAV, feeds, and health bypass tinyauth via auth.publicPaths.
{...}: {
  flake.modules.nixos.vikunja-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.vikunja;
  in {
    config.neo.services.vikunja.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      proxyPass = "http://host.docker.internal:${toString cfg.port}/";
    });
  };
}
