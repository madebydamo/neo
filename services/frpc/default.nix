{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.neo.services.frpc;
  frpcConfig = pkgs.writeText "frpc.ini" ''
    [common]
    server_addr = ${cfg.serverAddr}
    server_port = ${toString cfg.serverPort}
    token = ${cfg.token}

    [filebrowser]
    type = https
    subdomain = filebrowser
    plugin = https2https
    plugin_local_addr = swag:443
    plugin_crt_path = ${cfg.certPath}
    plugin_key_path = ${cfg.keyPath}
    plugin_host_header_rewrite = 127.0.0.1
  '';
in
{
  imports = [ ./option.nix ];

  systemd.tmpfiles.rules = mkIf cfg.enabled [
    "d ${config.neo.volumes.appdata}/frpc 0755 0 0 -"
  ];
}
// (mkIf (cfg.enabled && config.neo.services.swag.enabled) {
  virtualisation.oci-containers.containers.frpc = {
    image = "fatedier/frp:latest";
    autoStart = true;
    volumes = [
      "${frpcConfig}:/etc/frp/frpc.ini:ro"
      "${cfg.certPath}:${cfg.certPath}:ro"
      "${cfg.keyPath}:${cfg.keyPath}:ro"
    ];
    cmd = [ "frpc" "-c" "/etc/frp/frpc.ini" ];
  };
})