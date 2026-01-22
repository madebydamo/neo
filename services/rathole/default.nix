{
  config,
  lib,
  pkgs,
  ...
}:

with lib;

let
  cfg = config.neo.services.rathole;
  configContent = ''
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
    system.activationScripts.create-rathole-dirs = lib.neo.mkActivationScriptForDir {
      dirPath = "${config.neo.volumes.appdata}/rathole";
      user = "1000";
      group = "1000";
    };

    system.activationScripts.rathole-config = lib.neo.mkActivationScriptForFile {
      filePath = "${config.neo.volumes.appdata}/rathole/config.toml";
      content = configContent;
      mode = "0644";
      user = "1000";
      group = "1000";
    };
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
