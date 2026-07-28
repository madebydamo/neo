{...}: {
  flake.modules.nixos.swag-streamproxy = {
    config,
    lib,
    ...
  }:
    with lib; let
      streamproxyEnabled = config.neo.services.streamproxy.enabled;
    in {
      config = mkIf streamproxyEnabled {
        neo.services.swag.localHttpPort = mkForce 9980;
        neo.services.swag.localHttpsPort = mkForce 9981;
        neo.services.swag.localHttpsProxyProtocolPort = mkForce 9982;
      };
    };
}
