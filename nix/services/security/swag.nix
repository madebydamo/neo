# Security report reverse proxy for SWAG (host neo-security-web on cfg.port).
# Binds 127.0.0.1; host.docker.internal reaches it via mkDockerToLocalhostForward.
# The whole page stays behind tinyauth.
{...}: {
  flake.modules.nixos.security-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.security;
  in {
    config.neo.services.security.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      proxyPass = "http://host.docker.internal:${toString cfg.port}/";
    });
  };
}
