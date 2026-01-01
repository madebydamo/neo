{
  config,
  lib,
  pkgs,
  ...
}:

with lib;

let
  cfg = config.neo.services.rathole;
  configFile = pkgs.writeText "rathole.toml" ''
    [client]
    remote_addr = "${cfg.remoteAddr}:${toString cfg.port}"

    [client.services.${cfg.name}_http]
    token = "${cfg.token}"
    local_addr = "swag:80"

    [client.services.${cfg.name}_https]
    token = "${cfg.token}"
    local_addr = "swag:443"
  '';
in
{
  imports = [
    ./option.nix
  ];

  config = mkIf cfg.enabled {
    systemd.tmpfiles.rules = [
      "d ${config.neo.volumes.appdata}/rathole 0755 1000 1000 -"
      "C+ ${config.neo.volumes.appdata}/rathole/config.toml - - - - ${configFile}"
    ];
    virtualisation.oci-containers.containers.rathole = {
      image = "rapiz1/rathole:latest";
      autoStart = true;
      volumes = [
        "${config.neo.volumes.appdata}/rathole:/config"
      ];
      cmd = [
        "--client"
        "/config/config.toml"
      ];
      extraOptions = [
        "--network=internal"
      ];
    };
  };
}
