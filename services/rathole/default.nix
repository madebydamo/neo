{
  config,
  lib,
  pkgs,
  ...
}:
with lib; let
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
in {
  imports = [
    ./option.nix
  ];

  config = mkIf cfg.enabled {
    system.activationScripts.create-rathole-dirs = lib.neo.mkActivationScriptForDir config {
      dirPath = "${config.neo.volumes.appdata}/rathole";
      user = toString config.neo.uid;
      group = toString config.neo.gid;
    };

    system.activationScripts.rathole-config = lib.neo.mkActivationScriptForFile config {
      filePath = "${config.neo.volumes.appdata}/rathole/config.toml";
      content = configContent;
      mode = "0644";
      user = toString config.neo.uid;
      group = toString config.neo.gid;
    };
    virtualisation.oci-containers.containers.rathole = {
      user = "${toString config.neo.uid}:${toString config.neo.gid}";
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
