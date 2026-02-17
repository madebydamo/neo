{
  config,
  lib,
  pkgs,
  ...
}:
with lib; let
  cfg = config.neo.services.rathole;
  configFile = pkgs.writeText "rathole-client.toml" ''
    [client]
    remote_addr = "${cfg.remoteAddr}:${toString cfg.port}"

    [client.services.${cfg.name}_http]
    token = "${cfg.token}"
    local_addr = "127.0.0.1:80"

    [client.services.${cfg.name}_https]
    token = "${cfg.token}"
    local_addr = "127.0.0.1:443"
  '';
in {
  imports = [
    ./option.nix
  ];

  config = mkIf cfg.enabled {
    systemd.services.rathole = {
      description = "Rathole client tunnel";
      after = ["network-online.target"];
      wants = ["network-online.target"];
      wantedBy = ["multi-user.target"];
      serviceConfig = {
        ExecStart = "${pkgs.rathole}/bin/rathole --client ${configFile}";
        Restart = "always";
        RestartSec = 5;
        DynamicUser = true;
      };
    };
  };
}
